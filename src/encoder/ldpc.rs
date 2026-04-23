/// Quasi-cyclic LDPC encoder/decoder.
///
/// Uses a systematic code: codeword = [data | parity].
/// The parity check matrix H = [A | I_m] where A is sparse.
/// Encoding: parity = A * data (mod 2).
/// Decoding: min-sum belief propagation, configurable iterations.
#[allow(clippy::needless_range_loop)]

use crate::config::EccLevel;

// ---------------------------------------------------------------------------
// Sparse binary matrix (CSR format)
// ---------------------------------------------------------------------------

pub struct SparseMat {
    /// Number of rows (check nodes), columns (variable nodes).
    pub rows: usize,
    pub cols: usize,
    /// row_ptr[i]..row_ptr[i+1] = column indices for row i.
    row_ptr: Vec<u32>,
    col_idx: Vec<u32>,
    /// col_ptr[j]..col_ptr[j+1] = row indices for column j.
    col_ptr: Vec<u32>,
    row_idx: Vec<u32>,
}

impl SparseMat {
    /// Generate the A sub-matrix (m × k) deterministically from (m, k, seed).
    fn generate_a(m: usize, k: usize, row_weight: usize) -> Vec<Vec<u32>> {
        let mut rows: Vec<Vec<u32>> = vec![vec![]; m];
        if m == 0 || k == 0 {
            return rows;
        }
        let col_weight = ((row_weight * m).max(1) + k - 1) / k;
        let col_weight = col_weight.max(1).min(m);
        for col in 0..k as u32 {
            let mut hits = 0usize;
            let mut row = ((col as usize * 7 + 3) % m) as u32;
            let step = if m > 1 { ((col as usize * 11 + 5) % (m - 1) + 1) as u32 } else { 1 };
            let mut guard = 0usize;
            while hits < col_weight && guard < m * 2 {
                if !rows[row as usize].contains(&col) {
                    rows[row as usize].push(col);
                    hits += 1;
                }
                row = (row + step) % m as u32;
                guard += 1;
            }
        }
        rows
    }

    /// Build full parity check matrix H = [A | I_m] from (k, m).
    pub fn build(k: usize, m: usize, row_weight: usize) -> Self {
        let n = k + m;
        let a_rows = Self::generate_a(m, k, row_weight);

        // Build row-major CSR for H
        let mut row_ptr = Vec::with_capacity(m + 1);
        let mut col_idx_vec: Vec<u32> = Vec::new();
        row_ptr.push(0u32);
        for (i, a_row) in a_rows.iter().enumerate() {
            for &c in a_row {
                col_idx_vec.push(c);
            }
            // Identity diagonal
            col_idx_vec.push((k + i) as u32);
            row_ptr.push(col_idx_vec.len() as u32);
        }

        // Build col-major CSR for H (transpose)
        let mut col_rows: Vec<Vec<u32>> = vec![vec![]; n];
        for (r, a_row) in a_rows.iter().enumerate() {
            for &c in a_row {
                col_rows[c as usize].push(r as u32);
            }
            col_rows[k + r].push(r as u32);
        }
        let mut col_ptr = Vec::with_capacity(n + 1);
        let mut row_idx_vec: Vec<u32> = Vec::new();
        col_ptr.push(0u32);
        for cr in &col_rows {
            for &r in cr {
                row_idx_vec.push(r);
            }
            col_ptr.push(row_idx_vec.len() as u32);
        }

        SparseMat {
            rows: m,
            cols: n,
            row_ptr,
            col_idx: col_idx_vec,
            col_ptr,
            row_idx: row_idx_vec,
        }
    }

    #[inline(always)]
    pub fn row_cols(&self, r: usize) -> &[u32] {
        unsafe {
            let s = *self.row_ptr.get_unchecked(r) as usize;
            let e = *self.row_ptr.get_unchecked(r + 1) as usize;
            self.col_idx.get_unchecked(s..e)
        }
    }

    #[inline(always)]
    pub fn col_rows(&self, c: usize) -> &[u32] {
        unsafe {
            let s = *self.col_ptr.get_unchecked(c) as usize;
            let e = *self.col_ptr.get_unchecked(c + 1) as usize;
            self.row_idx.get_unchecked(s..e)
        }
    }
}

// ---------------------------------------------------------------------------
// LDPC Codec
// ---------------------------------------------------------------------------

/// Row weight for the sparse A matrix (check-node degree).
const ROW_WEIGHT: usize = 6;
/// Max belief propagation iterations.
const MAX_ITER: usize = 30;
/// Min-sum attenuation factor (0.75 is typical).
const ATTN: f32 = 0.75;

pub struct LdpcCodec {
    /// Number of data bits.
    pub k: usize,
    /// Number of encoded bits (k + parity).
    pub n: usize,
    h: SparseMat,
}

impl LdpcCodec {
    /// Create codec for encoding `data_bits` bits at the given ECC level.
    pub fn new(data_bits: usize, ecc: EccLevel) -> Self {
        let (num, den) = ecc.rate();
        let n = (data_bits * den + num - 1) / num;
        let m = n - data_bits;
        let h = SparseMat::build(data_bits, m, ROW_WEIGHT);
        LdpcCodec { k: data_bits, n, h }
    }

    /// Create codec from known codeword length `n_bits` and ECC level (decoder side).
    pub fn new_from_n(n_bits: usize, ecc: EccLevel) -> Self {
        let (num, den) = ecc.rate();
        let k = n_bits * num / den;
        let m = n_bits - k;
        let h = SparseMat::build(k, m, ROW_WEIGHT);
        LdpcCodec { k, n: n_bits, h }
    }

    // -----------------------------------------------------------------------
    // Encoding
    // -----------------------------------------------------------------------

    /// Encode `data` bits (length self.k) → codeword bits (length self.n).
    /// Uses systematic form: codeword = [data | parity].
    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        debug_assert_eq!(data.len(), (self.k + 7) / 8);
        let m = self.n - self.k;

        // Compute parity = A * data (mod 2)
        let mut parity = vec![0u8; (m + 7) / 8];
        for row in 0..m {
            let mut bit = 0u8;
            for &col in self.h.row_cols(row) {
                if (col as usize) < self.k {
                    // A part
                    bit ^= get_bit(data, col as usize);
                }
                // Identity part (col = k + row) is handled separately
            }
            set_bit(&mut parity, row, bit);
        }

        // Codeword: data bytes + parity bytes
        let mut codeword = Vec::with_capacity((self.n + 7) / 8);
        codeword.extend_from_slice(data);
        codeword.extend_from_slice(&parity);
        codeword
    }

    // -----------------------------------------------------------------------
    // Decoding (min-sum belief propagation)
    // -----------------------------------------------------------------------

    /// Decode `received` bits (length self.n) → best-effort data bits (length self.k).
    /// `channel_llr` provides per-bit log-likelihood ratios (+∞ = confident 0, -∞ = confident 1).
    /// If `channel_llr` is None, hard-decision from `received` is used.
    pub fn decode(&self, received: &[u8], channel_llr: Option<&[f32]>) -> Vec<u8> {
        let n = self.n;
        let m = self.n - self.k;
        let h = &self.h;

        // Initialize variable node LLRs from channel
        // Use hard-decision fallback if LLR array is shorter than expected (mismatched ModulePx)
        let channel: Vec<f32> = (0..n)
            .map(|i| {
                if let Some(llr) = channel_llr {
                    if i < llr.len() {
                        llr[i]
                    } else {
                        // Fallback: use hard-decision from received bits
                        if get_bit(received, i) == 0 { 10.0 } else { -10.0 }
                    }
                } else {
                    if get_bit(received, i) == 0 { 10.0 } else { -10.0 }
                }
            })
            .collect();

        // Check-to-variable messages c2v[edge_start(r) + i]
        let c2v_starts: Vec<usize> = {
            let mut starts = Vec::with_capacity(m + 1);
            let mut acc = 0usize;
            for r in 0..m {
                starts.push(acc);
                acc += h.row_cols(r).len();
            }
            starts.push(acc);
            starts
        };
        let total_edges = c2v_starts[m];
        let mut c2v = vec![0.0f32; total_edges];

        // Variable-to-check messages (same layout)
        let mut v2c = vec![0.0f32; total_edges];

        // Per-variable: sum of all incoming c2v messages
        let mut v_sum = vec![0.0f32; n];

        // edge_start helper
        let edge_start = |r: usize| c2v_starts[r];

        for _iter in 0..MAX_ITER {
            // --- Variable → Check update ---
            // v2c[r][i] = channel[v] + v_sum[v] - c2v[r][i]
            for r in 0..m {
                let cols = h.row_cols(r);
                let es = edge_start(r);
                for (i, &col) in cols.iter().enumerate() {
                    let v = col as usize;
                    v2c[es + i] = channel[v] + v_sum[v] - c2v[es + i];
                }
            }

            // --- Check → Variable update (min-sum) ---
            // Reset v_sum for new round
            v_sum.fill(0.0);

            for r in 0..m {
                let cols = h.row_cols(r);
                let es = edge_start(r);
                let len = cols.len();
                if len == 0 { continue; }

                // Compute product of signs and two smallest magnitudes
                let mut prod_sign = 1.0f32;
                let mut min1 = f32::INFINITY;
                let mut min2 = f32::INFINITY;
                for i in 0..len {
                    let val = v2c[es + i];
                    prod_sign *= val.signum();
                    let abs = val.abs();
                    if abs < min1 {
                        min2 = min1;
                        min1 = abs;
                    } else if abs < min2 {
                        min2 = abs;
                    }
                }

                for i in 0..len {
                    let val = v2c[es + i];
                    let sign_excl = prod_sign * val.signum();
                    let abs_val = val.abs();
                    let mag_excl = if abs_val <= min1 { min2 } else { min1 };
                    let msg = sign_excl * mag_excl * ATTN;
                    c2v[es + i] = msg;
                    // Add to v_sum for variable cols[i]
                    v_sum[cols[i] as usize] += msg;
                }
            }

            // --- Hard decision & syndrome check ---
            let mut converged = true;
            for r in 0..m {
                let mut s = 0u8;
                for &c in h.row_cols(r) {
                    let v = c as usize;
                    let llr = channel[v] + v_sum[v];
                    s ^= (llr < 0.0) as u8;
                }
                if s != 0 {
                    converged = false;
                    break;
                }
            }
            if converged {
                let hard: Vec<u8> = (0..self.k).map(|i| ((channel[i] + v_sum[i]) < 0.0) as u8).collect();
                return bits_to_bytes(&hard, self.k);
            }
        }

        // Did not converge — return best hard decision anyway
        let hard: Vec<u8> = (0..self.k).map(|i| ((channel[i] + v_sum[i]) < 0.0) as u8).collect();
        bits_to_bytes(&hard, self.k)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline(always)]
fn get_bit(buf: &[u8], pos: usize) -> u8 {
    unsafe {
        (*buf.get_unchecked(pos >> 3) >> (pos & 7)) & 1
    }
}

#[inline(always)]
fn set_bit(buf: &mut [u8], pos: usize, val: u8) {
    unsafe {
        let byte = buf.get_unchecked_mut(pos >> 3);
        let mask = 1u8 << (pos & 7);
        *byte = (*byte & !mask) | ((val & 1) << (pos & 7));
    }
}

#[inline(always)]
fn bits_to_bytes(bits: &[u8], k: usize) -> Vec<u8> {
    let mut out = vec![0u8; (k + 7) >> 3];
    for i in 0..k {
        unsafe {
            if *bits.get_unchecked(i) != 0 {
                *out.get_unchecked_mut(i >> 3) |= 1 << (i & 7);
            }
        }
    }
    out
}

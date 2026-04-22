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

#[derive(Clone)]
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
const MAX_ITER: usize = 50;
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

        // Initialize variable node LLRs from channel
        let mut v_llr: Vec<f32> = (0..n)
            .map(|i| {
                if let Some(llr) = channel_llr {
                    llr[i]
                } else {
                    // Hard decision: 0 → +10.0, 1 → -10.0
                    if get_bit(received, i) == 0 { 10.0 } else { -10.0 }
                }
            })
            .collect();

        // Check-to-variable messages c2v[r][idx] (stored flat per check node)
        let c2v_starts: Vec<usize> = (0..=m)
            .map(|r| if r < m { self.h.row_cols(r).len() } else { 0 })
            .scan(0usize, |acc, x| { let s = *acc; *acc += x; Some(s) })
            .collect();
        let total_edges: usize = c2v_starts[m];
        let mut c2v = vec![0.0f32; total_edges];

        // Map (row, col_idx_in_row) → edge index
        let edge_start = |r: usize| c2v_starts[r];

        // v2c scratch: variable-to-check per check node
        let mut v2c = vec![0.0f32; total_edges];

        let channel: Vec<f32> = v_llr.clone();

        for _iter in 0..MAX_ITER {
            // --- Variable → Check update ---
            // For each check node r, compute v2c[r][i] = L_ch[v] + sum of c2v except from r
            for r in 0..m {
                let cols = self.h.row_cols(r);
                let es = edge_start(r);
                for (i, &col) in cols.iter().enumerate() {
                    let v = col as usize;
                    // sum all c2v messages arriving at v except from r
                    let sum_all: f32 = self
                        .h
                        .col_rows(v)
                        .iter()
                        .enumerate()
                        .map(|(_, &r2)| {
                            let r2 = r2 as usize;
                            // find edge index of v in row r2
                            let cols2 = self.h.row_cols(r2);
                            let pos = cols2.iter().position(|&c| c as usize == v).unwrap_or(0);
                            c2v[edge_start(r2) + pos]
                        })
                        .sum();
                    // subtract self
                    v2c[es + i] = channel[v] + sum_all - c2v[es + i];
                }
            }

            // --- Check → Variable update (min-sum) ---
            for r in 0..m {
                let cols = self.h.row_cols(r);
                let es = edge_start(r);
                let len = cols.len();
                // min-sum: product of signs, minimum of magnitudes
                let prod_sign: f32 = v2c[es..es + len].iter().map(|x| x.signum()).product();
                let min_mag: f32   = v2c[es..es + len].iter().map(|x| x.abs()).fold(f32::INFINITY, f32::min);
                for i in 0..len {
                    let sign_excl = prod_sign * v2c[es + i].signum();
                    let mag_excl  = min_mag_excluding(&v2c[es..es + len], i) * ATTN;
                    c2v[es + i] = sign_excl * mag_excl;
                }
            }

            // --- Posterior LLR update ---
            for v in 0..n {
                let sum: f32 = self
                    .h
                    .col_rows(v)
                    .iter()
                    .map(|&r| {
                        let r = r as usize;
                        let cols = self.h.row_cols(r);
                        let pos = cols.iter().position(|&c| c as usize == v).unwrap_or(0);
                        c2v[edge_start(r) + pos]
                    })
                    .sum();
                v_llr[v] = channel[v] + sum;
            }

            // --- Hard decision & syndrome check ---
            let hard: Vec<u8> = (0..n).map(|i| (v_llr[i] < 0.0) as u8).collect();
            if syndrome_ok(&self.h, &hard, m) {
                // Extract data bits (systematic: first k bits)
                return bits_to_bytes(&hard, self.k);
            }
        }

        // Did not converge — return best hard decision anyway
        let hard: Vec<u8> = (0..self.k).map(|i| (v_llr[i] < 0.0) as u8).collect();
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
fn syndrome_ok(h: &SparseMat, bits: &[u8], m: usize) -> bool {
    for r in 0..m {
        let mut s = 0u8;
        for &c in h.row_cols(r) {
            s ^= unsafe { *bits.get_unchecked(c as usize) };
        }
        if s != 0 {
            return false;
        }
    }
    true
}

#[inline(always)]
fn min_mag_excluding(slice: &[f32], exclude: usize) -> f32 {
    let mut min = f32::INFINITY;
    for (i, x) in slice.iter().enumerate() {
        if i != exclude {
            let abs = x.abs();
            if abs < min {
                min = abs;
            }
        }
    }
    min
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

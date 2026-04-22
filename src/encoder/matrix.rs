use crate::{
    config::JabConfig,
    encoder::pattern::{self, FP_SIZE, AP_SIZE},
};

/// Minimum symbol side in modules.
pub const MIN_SIDE: usize = 21;

/// Compute the smallest square symbol side that fits `encoded_bits` bits
/// at `bpm` bits per module (after reserving overhead).
pub fn required_side(encoded_bits: usize, bpm: u8) -> usize {
    let mut side = MIN_SIDE;
    loop {
        let ap = pattern::alignment_positions(side).len();
        // Reserved: 4 FPs + APs + metadata strip (2 rows * (side - 2*FP_SIZE) cols)
        let metadata_cols = side.saturating_sub(2 * FP_SIZE);
        let reserved = 4 * FP_SIZE * FP_SIZE + ap * AP_SIZE * AP_SIZE + 2 * metadata_cols;
        let total_modules = side * side;
        let usable = total_modules.saturating_sub(reserved);
        if usable * bpm as usize >= encoded_bits {
            return side;
        }
        side += 4; // grow by 4 to keep module placement tidy
    }
}

/// A single JAB code symbol: flat u8 module matrix (color indices) + metadata.
#[derive(Clone)]
pub struct JabMatrix {
    pub modules: Vec<u8>, // row-major, color indices
    pub side:    usize,
    pub bpm:     u8,
}

impl JabMatrix {
    pub fn new(side: usize, bpm: u8) -> Self {
        Self { modules: vec![0u8; side * side], side, bpm }
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, v: u8) {
        self.modules[r * self.side + c] = v;
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> u8 {
        self.modules[r * self.side + c]
    }
}

/// Build a complete JAB symbol matrix from encoded module values.
///
/// `encoded_modules` – slice of color indices (already LDPC-encoded + converted
/// from bits using `bpm` bits-per-module).
/// `data_len` – original data length in bytes (stored in metadata for decoder).
pub fn build_matrix(cfg: &JabConfig, encoded_modules: &[u8], data_len: usize) -> JabMatrix {
    let bpm  = cfg.colors.bits_per_module();
    let bits = encoded_modules.len() * bpm as usize;
    let side = required_side(bits, bpm);
    let mut m = JabMatrix::new(side, bpm);

    // Place finder patterns at corners
    pattern::place_fp(&mut m.modules, side, 0,              0,              0);
    pattern::place_fp(&mut m.modules, side, 0,              side - FP_SIZE, 1);
    pattern::place_fp(&mut m.modules, side, side - FP_SIZE, 0,              2);
    pattern::place_fp(&mut m.modules, side, side - FP_SIZE, side - FP_SIZE, 3);

    // Alignment patterns
    let ap_pos = pattern::alignment_positions(side);
    for &(ar, ac) in &ap_pos {
        pattern::place_ap(&mut m.modules, side, ar, ac);
    }

    // Color palette pattern (metadata)
    pattern::place_cpp(&mut m.modules, side, cfg.colors.count());

    // Store original data length in bytes
    pattern::place_data_len(&mut m.modules, side, data_len);

    // Fill data modules in raster order (row by row, left to right),
    // skipping reserved areas.
    let mut data_iter = encoded_modules.iter();
    'outer: for r in 0..side {
        for c in 0..side {
            if pattern::is_reserved(r, c, side, &ap_pos) {
                continue;
            }
            match data_iter.next() {
                Some(&v) => m.set(r, c, v),
                None     => break 'outer,
            }
        }
    }

    m
}

/// Render a JabMatrix to an RGBA pixel buffer.
/// `palette` must have at least 2^bpm entries.
pub fn render_rgba(mat: &JabMatrix, module_px: u32, palette: &[crate::color::Rgb]) -> Vec<u8> {
    let side_px = mat.side * module_px as usize;
    let mut pixels = vec![255u8; side_px * side_px * 4]; // RGBA
    let mpx = module_px as usize;
    for r in 0..mat.side {
        let pr = r * mpx;
        for c in 0..mat.side {
            let color_idx = mat.get(r, c) as usize;
            let rgb = palette.get(color_idx).copied().unwrap_or(crate::color::Rgb(0,0,0));
            let pc = c * mpx;
            let (rv, gv, bv) = (rgb.0, rgb.1, rgb.2);
            for dy in 0..mpx {
                let row_start = ((pr + dy) * side_px + pc) * 4;
                for dx in 0..mpx {
                    let pix = row_start + dx * 4;
                    pixels[pix]     = rv;
                    pixels[pix + 1] = gv;
                    pixels[pix + 2] = bv;
                    pixels[pix + 3] = 255;
                }
            }
        }
    }
    pixels
}

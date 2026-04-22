/// JAB code finder and alignment pattern generation.
///
/// Each of the 4 finder patterns (FP0-FP3) is a 7×7 concentric square.
/// The color ordering of each FP differs to encode orientation.
use crate::color::Rgb;

pub const FP_SIZE: usize = 7;
pub const AP_SIZE: usize = 3;

/// Color indices (into palette) for each of the 4 FPs.
/// Layer 0 = outermost ring, layer 3 = center pixel.
/// Each FP has a unique permutation of colors 0-3.
static FP_COLORS: [[u8; 4]; 4] = [
    [0, 1, 2, 3], // FP0: top-left      (black, cyan, magenta, yellow)
    [0, 3, 2, 1], // FP1: top-right     (rotated)
    [0, 2, 1, 3], // FP2: bottom-left   (rotated)
    [0, 1, 3, 2], // FP3: bottom-right  (rotated)
];

/// Returns the color index at module (row, col) within a 7×7 FP.
#[inline]
pub fn fp_color(fp_idx: usize, row: usize, col: usize) -> u8 {
    // "Ring" = distance from center (3,3), clamped to 0-3
    let dr = (row as isize - 3).unsigned_abs();
    let dc = (col as isize - 3).unsigned_abs();
    let ring = dr.max(dc).min(3);
    FP_COLORS[fp_idx][ring]
}

/// Write finder pattern into a flat module matrix.
/// `mat` is row-major, width `w`. FP placed at top-left corner (or_r, or_c).
pub fn place_fp(mat: &mut [u8], w: usize, or_r: usize, or_c: usize, fp_idx: usize) {
    for r in 0..FP_SIZE {
        for c in 0..FP_SIZE {
            mat[(or_r + r) * w + (or_c + c)] = fp_color(fp_idx, r, c);
        }
    }
}

/// Write a 3×3 alignment pattern (color 0 border, color 1 center) at (or_r, or_c).
pub fn place_ap(mat: &mut [u8], w: usize, or_r: usize, or_c: usize) {
    for r in 0..AP_SIZE {
        for c in 0..AP_SIZE {
            let color = if r == 1 && c == 1 { 1u8 } else { 0u8 };
            mat[(or_r + r) * w + (or_c + c)] = color;
        }
    }
}

/// Calculate alignment pattern positions for a symbol of size (side × side).
/// APs are spaced every ~20 modules, avoiding FP areas.
pub fn alignment_positions(side: usize) -> Vec<(usize, usize)> {
    if side <= 21 {
        return vec![];
    }
    let spacing = 20usize;
    let mut positions = Vec::new();
    let mut r = FP_SIZE + 2;
    while r + AP_SIZE <= side - FP_SIZE - 2 {
        let mut c = FP_SIZE + 2;
        while c + AP_SIZE <= side - FP_SIZE - 2 {
            positions.push((r, c));
            c += spacing;
        }
        r += spacing;
    }
    positions
}

/// Returns true if module (r, c) belongs to any finder/alignment pattern or metadata.
pub fn is_reserved(r: usize, c: usize, side: usize, ap_pos: &[(usize, usize)]) -> bool {
    // Four FPs at corners
    let in_fp = |or: usize, oc: usize| {
        r >= or && r < or + FP_SIZE && c >= oc && c < oc + FP_SIZE
    };
    if in_fp(0, 0)
        || in_fp(0, side - FP_SIZE)
        || in_fp(side - FP_SIZE, 0)
        || in_fp(side - FP_SIZE, side - FP_SIZE)
    {
        return true;
    }
    // Metadata strip: rows 0-1, columns FP_SIZE+1 to FP_SIZE+20
    if r <= 1 && c >= FP_SIZE + 1 && c < FP_SIZE + 21 {
        return true;
    }
    // Alignment patterns
    for &(ar, ac) in ap_pos {
        if r >= ar && r < ar + AP_SIZE && c >= ac && c < ac + AP_SIZE {
            return true;
        }
    }
    false
}

/// Place the color palette pattern (CPP) in the metadata strip next to FP0.
/// It's a 1×N strip starting at (0, FP_SIZE + 1) encoding the palette size.
pub fn place_cpp(mat: &mut [u8], w: usize, color_count: usize) {
    // Encode log2(color_count) as a small int in modules 7..11 of row 0
    let bpm = (color_count as u32).trailing_zeros() as u8; // 2..8
    for i in 0..4usize {
        let bit = (bpm >> i) & 1;
        mat[FP_SIZE + 1 + i] = if bit == 1 { 1 } else { 0 };
    }
}

/// Place the data length (original data bytes count) in metadata strip row 1.
/// Uses modules (1, FP_SIZE+1) to (1, FP_SIZE+16) — 16 bits = up to 65535 bytes.
pub fn place_data_len(mat: &mut [u8], w: usize, data_byte_count: usize) {
    let len = data_byte_count as u16;
    for i in 0..16usize {
        let bit = (len >> i) & 1;
        mat[w + FP_SIZE + 1 + i] = if bit == 1 { 1 } else { 0 };
    }
}

/// Read the data length (original data bytes) from metadata strip row 1.
pub fn read_data_len(mat: &[u8], w: usize) -> usize {
    let mut len = 0u16;
    for i in 0..16usize {
        len |= ((mat[w + FP_SIZE + 1 + i] & 1) as u16) << i;
    }
    len as usize
}

/// Decode color count from the CPP strip.
pub fn read_cpp(mat: &[u8], _w: usize) -> u8 {
    let mut bpm = 0u8;
    for i in 0..4usize {
        bpm |= (mat[FP_SIZE + 1 + i] & 1) << i;
    }
    bpm.max(2).min(8)
}

/// Returns the palette Rgb values for standard JAB finder patterns
/// (first 4 colors are always used regardless of palette size).
pub fn fp_palette_colors(palette: &[Rgb]) -> [Rgb; 4] {
    [palette[0], palette[1], palette[2], palette[3]]
}

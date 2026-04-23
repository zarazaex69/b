use crate::color::{nearest_color, Rgb};
use crate::encoder::pattern::{FP_SIZE, fp_color};


/// Detected finder pattern location in pixel space.
#[derive(Debug, Clone, Copy)]
pub struct FpLocation {
    pub fp_idx: usize,
    pub row_px: u32,
    pub col_px: u32,
}

/// Given an RGBA image and a palette, find the four finder patterns.
/// Returns `None` if fewer than 4 FPs are found.
pub fn detect_fps(
    rgba: &[u8],
    width: u32,
    height: u32,
    palette: &[Rgb],
    module_px: u32,
) -> Option<[FpLocation; 4]> {
    let fp_side_px = FP_SIZE as u32 * module_px;
    let step = module_px;

    let mut found: Vec<FpLocation> = Vec::with_capacity(4);

    // Scan the image in coarse steps; try each candidate top-left corner.
    let mut row_px = 0u32;
    while row_px + fp_side_px <= height && found.len() < 4 {
        let mut col_px = 0u32;
        while col_px + fp_side_px <= width {
            // Try all 4 FP orientations
            for fp_idx in 0..4 {
                if matches_fp(rgba, width, row_px, col_px, module_px, palette, fp_idx) {
                    // Avoid duplicates (within 2*FP_SIZE modules)
                    let dup = found.iter().any(|f| {
                        (f.row_px as i64 - row_px as i64).unsigned_abs() < fp_side_px as u64
                            && (f.col_px as i64 - col_px as i64).unsigned_abs() < fp_side_px as u64
                    });
                    if !dup {
                        found.push(FpLocation { fp_idx, row_px, col_px });
                    }
                }
            }
            col_px += step;
        }
        row_px += step;
    }

    if found.len() < 4 {
        return None;
    }
    

    // Sort into [FP0, FP1, FP2, FP3] by fp_idx
    let mut arr = [found[0], found[0], found[0], found[0]];
    for loc in &found {
        if loc.fp_idx < 4 {
            arr[loc.fp_idx] = *loc;
        }
    }
    Some(arr)
}

/// Check whether the region at (row_px, col_px) matches FP `fp_idx`.
fn matches_fp(
    rgba: &[u8],
    img_width: u32,
    row_px: u32,
    col_px: u32,
    module_px: u32,
    palette: &[Rgb],
    fp_idx: usize,
) -> bool {
    let mut mismatches = 0usize;
    for mr in 0..FP_SIZE {
        for mc in 0..FP_SIZE {
            let expected_color = fp_color(fp_idx, mr, mc);
            let _expected_rgb  = palette[expected_color as usize];
            // Sample center pixel of module
            let px = col_px + mc as u32 * module_px + module_px / 2;
            let py = row_px + mr as u32 * module_px + module_px / 2;
            if px >= img_width { return false; }
            let idx  = (py * img_width + px) as usize * 4;
            let obs  = Rgb(rgba[idx], rgba[idx + 1], rgba[idx + 2]);
            let nearest = nearest_color(palette, obs);
            if nearest != expected_color {
                mismatches += 1;
                if mismatches > 3 { // tolerate minor noise
                    return false;
                }
            }
        }
    }
    true
}

/// Detect module_px by trying candidate scales and validating with detect_fps.
/// Scans from max_px down to min_px, returns first scale where 4 FPs are found.
pub fn detect_module_px(
    rgba: &[u8],
    width: u32,
    height: u32,
    palette: &[Rgb],
    min_px: u32,
    max_px: u32,
) -> Option<u32> {
    for candidate_px in (min_px..=max_px).rev() {
        if detect_fps(rgba, width, height, palette, candidate_px).is_some() {
            return Some(candidate_px);
        }
    }
    None
}

/// Given four FP pixel locations, determine the module grid origin and size.
pub struct GridGeometry {
    pub origin_row_px: u32,
    pub origin_col_px: u32,
    pub side:          usize,
    pub module_px:     u32,
}

pub fn compute_geometry(fps: &[FpLocation; 4], module_px: u32) -> GridGeometry {
    let min_r = fps.iter().map(|f| f.row_px).min().unwrap_or(0);
    let min_c = fps.iter().map(|f| f.col_px).min().unwrap_or(0);
    let max_r = fps.iter().map(|f| f.row_px).max().unwrap_or(0);
    let max_c = fps.iter().map(|f| f.col_px).max().unwrap_or(0);

    // Distance between top-left and bottom-right FP gives symbol side
    let side_px = (max_r.max(max_c) - min_r.min(min_c)) as usize + FP_SIZE * module_px as usize;
    let side    = (side_px + module_px as usize / 2) / module_px as usize;

    GridGeometry {
        origin_row_px: min_r,
        origin_col_px: min_c,
        side,
        module_px,
    }
}

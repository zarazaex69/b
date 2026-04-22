use crate::{
    color::{nearest_color, Rgb},
    decoder::detect::GridGeometry,
    encoder::pattern,
};

/// Read the data length from the metadata strip in the image.
pub fn read_data_len_from_image(
    rgba: &[u8],
    img_width: u32,
    geo: &GridGeometry,
    palette: &[Rgb],
) -> usize {
    let mod_px = geo.module_px;
    let side = geo.side;
    let max_col = side.saturating_sub(pattern::FP_SIZE);
    let mut len = 0u16;
    for i in 0..16usize {
        let c = pattern::FP_SIZE + 1 + i;
        if c >= max_col { break; }
        let r = 1usize;
        let px = geo.origin_col_px + c as u32 * mod_px + mod_px / 2;
        let py = geo.origin_row_px + r as u32 * mod_px + mod_px / 2;
        let idx = (py * img_width + px) as usize * 4;
        if idx + 2 < rgba.len() {
            let obs = Rgb(rgba[idx], rgba[idx + 1], rgba[idx + 2]);
            let color = nearest_color(palette, obs);
            if color != 0 {
                len |= 1 << i;
            }
        }
    }
    len as usize
}

/// Sample the color index for every non-reserved module in the grid.
/// Returns a flat vec of color indices in raster order (matching encoder fill order).
pub fn sample_modules(
    rgba: &[u8],
    img_width: u32,
    geo: &GridGeometry,
    palette: &[Rgb],
) -> Vec<u8> {
    let side    = geo.side;
    let mod_px  = geo.module_px;
    let ap_pos  = pattern::alignment_positions(side);
    let mut out = Vec::with_capacity(side * side);

    // Precompute i32 palette for faster distance calc
    let palette_i32: Vec<(i32, i32, i32)> = palette.iter().map(|c| (c.0 as i32, c.1 as i32, c.2 as i32)).collect();

    for r in 0..side {
        for c in 0..side {
            if pattern::is_reserved(r, c, side, &ap_pos) {
                continue;
            }
            let px = geo.origin_col_px + c as u32 * mod_px + mod_px / 2;
            let py = geo.origin_row_px + r as u32 * mod_px + mod_px / 2;
            if px >= img_width {
                out.push(0);
                continue;
            }
            let idx  = (py * img_width + px) as usize * 4;
            if idx + 2 >= rgba.len() {
                out.push(0);
                continue;
            }
            let (obsr, obsg, obsb) = (rgba[idx] as i32, rgba[idx + 1] as i32, rgba[idx + 2] as i32);
            let mut best_idx = 0u8;
            let mut best_dist = i32::MAX;
            for (i, &(pr, pg, pb)) in palette_i32.iter().enumerate() {
                let dr = pr - obsr;
                let dg = pg - obsg;
                let db = pb - obsb;
                let d = dr * dr + dg * dg + db * db;
                if d < best_dist {
                    best_dist = d;
                    best_idx = i as u8;
                    if d == 0 { break; }
                }
            }
            out.push(best_idx);
        }
    }
    out
}

/// Convert a sampled module stream (color indices) back to a bit vector.
pub fn modules_to_bits(modules: &[u8], bpm: u8) -> Vec<u8> {
    let total_bits = modules.len() * bpm as usize;
    let mut out = vec![0u8; (total_bits + 7) / 8];
    for (i, &m) in modules.iter().enumerate() {
        let bit_off = i * bpm as usize;
        for b in 0..bpm as usize {
            if (m >> b) & 1 == 1 {
                out[bit_off / 8 + b / 8] |= 1 << ((bit_off + b) % 8);
            }
        }
    }
    out
}

/// Compute soft LLR values for each encoded bit from color distances.
/// Closer to intended color → higher confidence (larger |LLR|).
pub fn modules_to_llr(
    modules: &[u8],
    rgba: &[u8],
    img_width: u32,
    geo: &GridGeometry,
    palette: &[Rgb],
    bpm: u8,
) -> Vec<f32> {
    let side   = geo.side;
    let mod_px = geo.module_px;
    let ap_pos = pattern::alignment_positions(side);
    let max_dist_sq = 255.0_f32 * 255.0 * 3.0_f32;
    let mut llrs: Vec<f32> = Vec::with_capacity(modules.len() * bpm as usize);

    let mut idx_m = 0usize;
    for r in 0..side {
        for c in 0..side {
            if pattern::is_reserved(r, c, side, &ap_pos) {
                continue;
            }
            let px = geo.origin_col_px + c as u32 * mod_px + mod_px / 2;
            let py = geo.origin_row_px + r as u32 * mod_px + mod_px / 2;
            let px_idx = if px < img_width {
                (py * img_width + px) as usize * 4
            } else {
                usize::MAX
            };

            let color_idx = if idx_m < modules.len() { modules[idx_m] } else { 0 };
            let expected_rgb = palette.get(color_idx as usize).copied().unwrap_or(Rgb(0,0,0));

            let confidence = if px_idx != usize::MAX && px_idx + 2 < rgba.len() {
                let obs = Rgb(rgba[px_idx], rgba[px_idx + 1], rgba[px_idx + 2]);
                let d = obs.distance_sq(expected_rgb) as f32;
                // Convert distance to confidence: 0 dist → +10, max dist → -10
                10.0 * (1.0 - 2.0 * d / max_dist_sq)
            } else {
                0.0 // unknown
            };

            // Emit one LLR per bit; sign determined by the bit value in color_idx
            for b in 0..bpm {
                let bit = (color_idx >> b) & 1;
                let llr = if bit == 0 { confidence } else { -confidence };
                llrs.push(llr);
            }
            idx_m += 1;
        }
    }
    llrs
}

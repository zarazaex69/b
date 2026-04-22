pub mod detect;
pub mod sample;

use crate::{
    color::build_palette,
    config::JabConfig,
    encoder::{ldpc::LdpcCodec, pattern},
    error::{JabError, Result},
};
use detect::{compute_geometry, detect_fps};
use sample::{modules_to_bits, modules_to_llr, read_data_len_from_image, sample_modules};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Decode an RGBA image produced by the encoder back into the original bytes.
pub fn decode(rgba: &[u8], width: u32, height: u32, cfg: &JabConfig) -> Result<Vec<u8>> {
    let palette = build_palette(cfg.colors);
    let bpm     = cfg.colors.bits_per_module();
    let mod_px  = cfg.module_size;

    // 1. Detect finder patterns
    let fps = detect_fps(rgba, width, height, &palette, mod_px)
        .ok_or(JabError::PatternNotFound)?;

    // 2. Determine grid geometry
    let geo = compute_geometry(&fps, mod_px);
    if geo.side < 21 {
        return Err(JabError::InvalidMatrix(geo.side as u32, geo.side as u32));
    }

    // 3. Read original data length from metadata
    let data_byte_count = read_data_len_from_image(rgba, width, &geo, &palette);

    // 4. Sample ALL modules (we need them all for LDPC)
    let modules = sample_modules(rgba, width, &geo, &palette);

    // 5. Recreate the LDPC codec with the SAME parameters as encoder
    let data_bits = data_byte_count * 8;
    let codec = LdpcCodec::new(data_bits, cfg.ecc);

    // 6. We need exactly codec.n bits worth of modules
    let needed_modules = (codec.n + bpm as usize - 1) / bpm as usize;
    let modules = if needed_modules <= modules.len() {
        &modules[..needed_modules]
    } else {
        &modules[..]
    };

    // 7. Compute soft LLRs for LDPC decoder
    let all_llrs = modules_to_llr(modules, rgba, width, &geo, &palette, bpm);

    // 8. Convert modules → raw encoded bytes
    let encoded_bytes = modules_to_bits(modules, bpm);

    // 9. LDPC decode
    let data = codec.decode(&encoded_bytes, Some(&all_llrs));

    // 10. Truncate to original data length
    Ok(data[..data_byte_count.min(data.len())].to_vec())
}

/// Decode multiple RGBA frames in parallel.
#[cfg(feature = "parallel")]
pub fn decode_parallel(
    frames: &[(&[u8], u32, u32)],
    cfg: &JabConfig,
) -> Vec<Result<Vec<u8>>> {
    frames
        .par_iter()
        .map(|(rgba, w, h)| decode(rgba, *w, *h, cfg))
        .collect()
}

#[cfg(not(feature = "parallel"))]
pub fn decode_parallel(
    frames: &[(&[u8], u32, u32)],
    cfg: &JabConfig,
) -> Vec<Result<Vec<u8>>> {
    frames.iter().map(|(rgba, w, h)| decode(rgba, *w, *h, cfg)).collect()
}

/// Decode directly from a JabMatrix (no image detection needed — ideal for testing).
pub fn decode_matrix(
    mat: &crate::encoder::matrix::JabMatrix,
    cfg: &JabConfig,
) -> Result<Vec<u8>> {
    let bpm     = cfg.colors.bits_per_module();
    let side    = mat.side;
    let ap_pos  = pattern::alignment_positions(side);

    // Read original data byte count from metadata strip
    let data_byte_count = pattern::read_data_len(&mat.modules, side);

    // Build LDPC codec with the SAME parameters as encoder
    let data_bits = data_byte_count * 8;
    let codec = LdpcCodec::new(data_bits, cfg.ecc);

    // We need exactly codec.n bits worth of modules
    let needed_modules = (codec.n + bpm as usize - 1) / bpm as usize;

    // Extract data modules in the same raster order as the encoder
    let mut all_modules: Vec<u8> = Vec::new();
    for r in 0..side {
        for c in 0..side {
            if !pattern::is_reserved(r, c, side, &ap_pos) {
                all_modules.push(mat.get(r, c));
            }
        }
    }

    let modules = if needed_modules <= all_modules.len() {
        &all_modules[..needed_modules]
    } else {
        &all_modules[..]
    };

    let encoded_bytes = sample::modules_to_bits(modules, bpm);
    let data = codec.decode(&encoded_bytes, None);

    // Truncate to original data length
    Ok(data[..data_byte_count.min(data.len())].to_vec())
}

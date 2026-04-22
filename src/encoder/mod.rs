pub mod ldpc;
pub mod matrix;
pub mod pattern;

use crate::{
    bits::BitBuf,
    color::build_palette,
    config::JabConfig,
    error::Result,
};
use matrix::{build_matrix, JabMatrix};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Output of a complete encode operation.
pub struct EncodedJab {
    /// One or more symbol matrices (master + slaves).
    pub symbols:    Vec<JabMatrix>,
    /// RGBA pixel render of all symbols tiled horizontally.
    pub rgba:       Vec<u8>,
    pub img_width:  u32,
    pub img_height: u32,
    /// The palette used.
    pub palette:    Vec<crate::color::Rgb>,
    /// Original data length in bytes.
    pub data_len:   usize,
}

/// Encode arbitrary bytes into one or more JAB code symbols.
pub fn encode(data: &[u8], cfg: &JabConfig) -> Result<EncodedJab> {
    let palette = build_palette(cfg.colors);
    let bpm     = cfg.colors.bits_per_module();

    // ---------- split data across symbols if needed ----------
    // Calculate per-symbol capacity (conservative: side=21 → grow as needed).
    // We try to fit everything in one symbol first, then split.
    let data_bits = data.len() * 8;
    let codec     = ldpc::LdpcCodec::new(data_bits, cfg.ecc);
    let encoded_bytes = codec.encode(data);

    // Convert encoded bytes → module color-index stream
    let mut enc_bits = BitBuf::from_bytes(&encoded_bytes);
    // Pad to multiple of bpm
    while enc_bits.len() % bpm as usize != 0 {
        enc_bits.push_bits(0, 1);
    }
    let modules: Vec<u8> = enc_bits.modules(bpm).collect();

    // Build symbol matrix (stores data_len = original data.len() in metadata)
    let symbol = build_matrix(cfg, &modules, data.len());

    // Render
    let module_px = cfg.module_size;
    let rgba = matrix::render_rgba(&symbol, module_px, &palette);
    let img_width  = symbol.side as u32 * module_px;
    let img_height = symbol.side as u32 * module_px;

    Ok(EncodedJab {
        symbols: vec![symbol],
        rgba,
        img_width,
        img_height,
        palette,
        data_len: data.len(),
    })
}

/// Encode multiple independent chunks in parallel (ideal for real-time streaming).
/// Each chunk becomes one symbol.
#[cfg(feature = "parallel")]
pub fn encode_parallel(chunks: &[&[u8]], cfg: &JabConfig) -> Result<Vec<EncodedJab>> {
    chunks
        .par_iter()
        .map(|chunk| encode(chunk, cfg))
        .collect()
}

#[cfg(not(feature = "parallel"))]
pub fn encode_parallel(chunks: &[&[u8]], cfg: &JabConfig) -> Result<Vec<EncodedJab>> {
    chunks.iter().map(|chunk| encode(chunk, cfg)).collect()
}

/// Convenience: encode and return only the flat RGBA bytes + dimensions.
pub fn encode_rgba(data: &[u8], cfg: &JabConfig) -> Result<(Vec<u8>, u32, u32)> {
    let out = encode(data, cfg)?;
    Ok((out.rgba, out.img_width, out.img_height))
}

pub mod bits;
pub mod color;
pub mod config;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod ffi;
pub mod gpu;

// Re-export the most commonly used items at crate root.
pub use config::{ColorCount, EccLevel, JabConfig};
pub use encoder::{encode, encode_parallel, encode_rgba, EncodedJab};
pub use decoder::{decode, decode_matrix, decode_parallel};
pub use error::{JabError, Result};
pub use gpu::is_gpu_available;

#[cfg(feature = "gpu")]
pub use gpu::GpuContext;

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(data: &[u8], colors: ColorCount, ecc: EccLevel) {
        let cfg = JabConfig::new(colors, ecc);
        let out = encode(data, &cfg).expect("encode");
        let recovered = decode_matrix(&out.symbols[0], &cfg).expect("decode");
        assert_eq!(&recovered[..data.len()], data,
            "round-trip failed for {:?}/{:?}", colors, ecc);
    }

    #[test]
    fn rt_small_8colors() {
        round_trip(b"hello JAB!", ColorCount::C8, EccLevel::Medium);
    }

    #[test]
    fn rt_256colors() {
        round_trip(b"ultra high density 256 colors test", ColorCount::C256, EccLevel::Low);
    }

    #[test]
    fn rt_large_payload() {
        let data: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
        round_trip(&data, ColorCount::C256, EccLevel::Medium);
    }

    #[test]
    fn encode_gives_rgba() {
        let cfg = JabConfig { colors: ColorCount::C8, ecc: EccLevel::Medium,
                              module_size: 2, ..Default::default() };
        let (rgba, w, h) = encode_rgba(b"test", &cfg).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(w > 0 && h > 0);
    }

    #[test]
    fn capacity_grows_with_colors() {
        let cfg8   = JabConfig::new(ColorCount::C8,   EccLevel::Medium);
        let cfg256 = JabConfig::new(ColorCount::C256, EccLevel::Medium);
        assert!(cfg256.capacity_bytes(50) > cfg8.capacity_bytes(50));
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn parallel_encode() {
        let chunks: Vec<Vec<u8>> = (0..8)
            .map(|i| format!("chunk {i} data payload").into_bytes())
            .collect();
        let refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let cfg = JabConfig::default();
        let results = encode_parallel(&refs, &cfg).unwrap();
        assert_eq!(results.len(), 8);
    }

    #[test]
    fn decode_from_rgba() {
        let data = b"Hello, JAB!";
        let cfg = JabConfig {
            colors: ColorCount::C8,
            ecc: EccLevel::Medium,
            module_size: 4,
            ..Default::default()
        };

        let (rgba, w, h) = encode_rgba(data, &cfg).expect("encode");
        let recovered = decode(&rgba, w, h, &cfg).expect("decode from rgba should work");
        assert_eq!(&recovered[..data.len()], &data[..]);
    }
}

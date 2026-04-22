use crate::error::{JabError, Result};

/// Number of colors in the JAB code palette.
/// Bits per module = log2(color_count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ColorCount {
    C4   = 4,
    C8   = 8,
    C16  = 16,
    C32  = 32,
    C64  = 64,
    C128 = 128,
    C256 = 256,
}

impl ColorCount {
    pub fn from_u32(n: u32) -> Result<Self> {
        match n {
            4   => Ok(Self::C4),
            8   => Ok(Self::C8),
            16  => Ok(Self::C16),
            32  => Ok(Self::C32),
            64  => Ok(Self::C64),
            128 => Ok(Self::C128),
            256 => Ok(Self::C256),
            _   => Err(JabError::InvalidColorCount(n)),
        }
    }

    #[inline(always)]
    pub fn bits_per_module(self) -> u8 {
        (self as u32).trailing_zeros() as u8
    }

    #[inline(always)]
    pub fn count(self) -> usize {
        self as usize
    }
}

/// LDPC error correction level. Higher = more redundancy, less data capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EccLevel {
    /// ~75% data, ~25% ECC (rate 3/4)
    Low,
    /// ~50% data, ~50% ECC (rate 1/2)
    Medium,
    /// ~33% data, ~67% ECC (rate 1/3)
    High,
    /// ~25% data, ~75% ECC (rate 1/4)
    Ultra,
}

impl EccLevel {
    /// Returns (data_numerator, total_denominator) for the code rate.
    pub fn rate(self) -> (usize, usize) {
        match self {
            Self::Low    => (3, 4),
            Self::Medium => (1, 2),
            Self::High   => (1, 3),
            Self::Ultra  => (1, 4),
        }
    }

    /// Encoded length in bits for `data_bits` input bits.
    pub fn encoded_len_bits(self, data_bits: usize) -> usize {
        let (num, den) = self.rate();
        data_bits * den / num
    }
}

/// Master configuration for encoding/decoding.
#[derive(Debug, Clone)]
pub struct JabConfig {
    /// Number of distinct colors per module.
    pub colors: ColorCount,
    /// Error correction level.
    pub ecc: EccLevel,
    /// Maximum number of symbols (master + slaves). 0 = auto.
    pub max_symbols: usize,
    /// Module size in pixels for rendering.
    pub module_size: u32,
    /// Use GPU acceleration if available.
    pub use_gpu: bool,
}

impl Default for JabConfig {
    fn default() -> Self {
        Self {
            colors:      ColorCount::C8,
            ecc:         EccLevel::Medium,
            max_symbols: 0,
            module_size: 4,
            use_gpu:     false,
        }
    }
}

impl JabConfig {
    pub fn new(colors: ColorCount, ecc: EccLevel) -> Self {
        Self { colors, ecc, ..Default::default() }
    }

    /// Maximum raw data bytes per module column-row symbol of given side.
    pub fn capacity_bytes(&self, side: usize) -> usize {
        let bpm  = self.colors.bits_per_module() as usize;
        let (num, den) = self.ecc.rate();
        // overhead: 4 × 7×7 finder patterns + alignment + metadata ≈ 15% of modules
        let usable = (side * side * 85 / 100).saturating_sub(0);
        usable * bpm * num / den / 8
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum JabError {
    #[error("invalid color count {0}: must be 4, 8, 16, 32, 64, 128, or 256")]
    InvalidColorCount(u32),

    #[error("data too large ({0} bytes), max for this config is {1} bytes")]
    DataTooLarge(usize, usize),

    #[error("symbol version overflow: data requires more than {0} symbols")]
    VersionOverflow(usize),

    #[error("decode failed: {0}")]
    DecodeError(String),

    #[error("ldpc: max iterations reached without convergence")]
    LdpcNoConverge,

    #[error("pattern detection failed: no valid finder patterns found")]
    PatternNotFound,

    #[error("invalid matrix dimensions: {0}x{1}")]
    InvalidMatrix(u32, u32),

    #[error("gpu not available: {0}")]
    GpuError(String),

    #[error("ffi: null pointer passed")]
    NullPointer,

    #[error("ffi: invalid utf-8 in string")]
    InvalidUtf8,
}

pub type Result<T> = std::result::Result<T, JabError>;

/// GPU acceleration for JAB code matrix operations.
///
/// Enabled with `--features gpu`. Falls back gracefully to CPU
/// when wgpu is not available or no compatible adapter exists.
///
/// Current GPU operations:
///   - Parallel matrix filling (one thread per module)
///   - RGBA rendering (one thread per pixel)
///   - Batch encode: many symbols simultaneously
#[cfg(feature = "gpu")]
mod wgpu_backend;

#[cfg(feature = "gpu")]
pub use wgpu_backend::{GpuContext, gpu_render_rgba, gpu_encode_batch};

/// Returns true when GPU support is compiled in and an adapter is available.
pub fn is_gpu_available() -> bool {
    #[cfg(feature = "gpu")]
    {
        wgpu_backend::probe_adapter()
    }
    #[cfg(not(feature = "gpu"))]
    false
}

// ---------------------------------------------------------------------------
// Stub types so the rest of the codebase compiles without the gpu feature.
// ---------------------------------------------------------------------------

#[cfg(not(feature = "gpu"))]
pub struct GpuContext;

#[cfg(not(feature = "gpu"))]
impl GpuContext {
    pub fn new_blocking() -> Option<Self> { None }
}

// ---------------------------------------------------------------------------
// WGSL compute shader source (embedded here so the build doesn't need extra
// file-system access at runtime).
// ---------------------------------------------------------------------------

pub const SHADER_ENCODE: &str = include_str!("../../shaders/encode.wgsl");

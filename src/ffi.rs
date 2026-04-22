/// C-compatible FFI layer for Go (and any other language with cgo/FFI).
///
/// Calling convention: caller owns all input buffers; library allocates output
/// buffers that **must** be freed with `jab_free`.
///
/// Go example:
/// ```go
/// data := []byte("hello, JAB!")
/// var outLen C.ulong
/// ptr := C.jab_encode((*C.uchar)(unsafe.Pointer(&data[0])),
///                     C.ulong(len(data)), 4, 1, 4, &outLen)
/// defer C.jab_free(ptr)
/// rgba := C.GoBytes(unsafe.Pointer(ptr), C.int(outLen))
/// ```
use std::slice;
use crate::{
    config::{ColorCount, EccLevel, JabConfig},
    encoder, decoder,
};

// ---------------------------------------------------------------------------
// Opaque result type
// ---------------------------------------------------------------------------

/// Result returned by `jab_encode_ex`. Free with `jab_result_free`.
#[repr(C)]
pub struct JabEncodeResult {
    pub rgba:       *mut u8,
    pub rgba_len:   u64,
    pub img_width:  u32,
    pub img_height: u32,
    pub error:      i32, // 0 = ok
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn color_count(n: u32) -> ColorCount {
    ColorCount::from_u32(n).unwrap_or(ColorCount::C8)
}

fn ecc_level(n: u32) -> EccLevel {
    match n {
        0 => EccLevel::Low,
        1 => EccLevel::Medium,
        2 => EccLevel::High,
        3 => EccLevel::Ultra,
        _ => EccLevel::Medium,
    }
}

// ---------------------------------------------------------------------------
// Encode
// ---------------------------------------------------------------------------

/// Encode `data_len` bytes of `data` into a JAB RGBA image.
///
/// # Parameters
/// - `data`        – pointer to input bytes
/// - `data_len`    – number of input bytes
/// - `colors`      – color count: 4, 8, 16, 32, 64, 128, or 256
/// - `ecc`         – ECC level: 0=Low, 1=Medium, 2=High, 3=Ultra
/// - `module_px`   – pixels per module (e.g. 4)
/// - `out_len`     – written with the number of bytes in the returned buffer
///
/// # Returns
/// Pointer to a heap-allocated RGBA buffer. Free with `jab_free`.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jab_encode(
    data:       *const u8,
    data_len:   u64,
    colors:     u32,
    ecc:        u32,
    module_px:  u32,
    out_width:  *mut u32,
    out_height: *mut u32,
    out_len:    *mut u64,
) -> *mut u8 {
    if data.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, data_len as usize) };
    let cfg = JabConfig {
        colors:      color_count(colors),
        ecc:         ecc_level(ecc),
        module_size: module_px.max(1),
        ..Default::default()
    };
    match encoder::encode_rgba(bytes, &cfg) {
        Ok((mut buf, w, h)) => {
            buf.shrink_to_fit();
            unsafe {
                if !out_width.is_null()  { *out_width  = w; }
                if !out_height.is_null() { *out_height = h; }
                *out_len = buf.len() as u64;
            }
            let ptr = buf.as_mut_ptr();
            std::mem::forget(buf);
            ptr
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Extended encode: returns a `JabEncodeResult` struct (avoids out-params).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jab_encode_ex(
    data:      *const u8,
    data_len:  u64,
    colors:    u32,
    ecc:       u32,
    module_px: u32,
) -> JabEncodeResult {
    let null_result = |error: i32| JabEncodeResult {
        rgba: std::ptr::null_mut(), rgba_len: 0,
        img_width: 0, img_height: 0, error,
    };

    if data.is_null() {
        return null_result(-1);
    }
    let bytes = unsafe { slice::from_raw_parts(data, data_len as usize) };
    let cfg = JabConfig {
        colors:      color_count(colors),
        ecc:         ecc_level(ecc),
        module_size: module_px.max(1),
        ..Default::default()
    };
    match encoder::encode_rgba(bytes, &cfg) {
        Ok((mut buf, w, h)) => {
            buf.shrink_to_fit();
            let rgba_len = buf.len() as u64;
            let rgba     = buf.as_mut_ptr();
            std::mem::forget(buf);
            JabEncodeResult { rgba, rgba_len, img_width: w, img_height: h, error: 0 }
        }
        Err(_) => null_result(-2),
    }
}

// ---------------------------------------------------------------------------
// Decode
// ---------------------------------------------------------------------------

/// Decode a JAB RGBA image back to the original bytes.
///
/// # Returns
/// Pointer to heap-allocated decoded bytes. Free with `jab_free`.
/// Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jab_decode(
    rgba:      *const u8,
    rgba_len:  u64,
    width:     u32,
    height:    u32,
    colors:    u32,
    ecc:       u32,
    module_px: u32,
    out_len:   *mut u64,
) -> *mut u8 {
    if rgba.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let pixels = unsafe { slice::from_raw_parts(rgba, rgba_len as usize) };
    let cfg = JabConfig {
        colors:      color_count(colors),
        ecc:         ecc_level(ecc),
        module_size: module_px.max(1),
        ..Default::default()
    };
    match decoder::decode(pixels, width, height, &cfg) {
        Ok(mut data) => {
            data.shrink_to_fit();
            unsafe { *out_len = data.len() as u64; }
            let ptr = data.as_mut_ptr();
            std::mem::forget(data);
            ptr
        }
        Err(_) => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// Memory management
// ---------------------------------------------------------------------------

/// Free a buffer returned by any `jab_*` function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jab_free(ptr: *mut u8, len: u64) {
    if ptr.is_null() { return; }
    unsafe { drop(Vec::from_raw_parts(ptr, len as usize, len as usize)); }
}

/// Free a `JabEncodeResult`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jab_result_free(r: JabEncodeResult) {
    unsafe { jab_free(r.rgba, r.rgba_len); }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Returns the library version string (null-terminated, static lifetime).
#[unsafe(no_mangle)]
pub extern "C" fn jab_version() -> *const u8 {
    b"0.1.0\0".as_ptr()
}

/// Returns 1 if GPU acceleration is available, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn jab_gpu_available() -> i32 {
    crate::gpu::is_gpu_available() as i32
}

/// Maximum data bytes encodable in one symbol for the given config.
#[unsafe(no_mangle)]
pub extern "C" fn jab_capacity(colors: u32, ecc: u32, symbol_side: u32) -> u64 {
    let cfg = JabConfig {
        colors: color_count(colors),
        ecc:    ecc_level(ecc),
        ..Default::default()
    };
    cfg.capacity_bytes(symbol_side as usize) as u64
}

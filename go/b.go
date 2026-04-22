// Package b provides Go bindings for the b Rust library.
//
// Build the shared library first:
//   cargo build --release
//   cp target/release/libb.so go/
//
// Then build Go:
//   go build -v ./go/...
package b

/*
#cgo LDFLAGS: -L. -lb -Wl,-rpath,.
#include <stdint.h>
#include <stdlib.h>

typedef struct {
    uint8_t* rgba;
    uint64_t rgba_len;
    uint32_t img_width;
    uint32_t img_height;
    int32_t  error;
} JabEncodeResult;

extern uint8_t* jab_encode(
    const uint8_t* data, uint64_t data_len,
    uint32_t colors, uint32_t ecc, uint32_t module_px,
    uint32_t* out_width, uint32_t* out_height, uint64_t* out_len);

extern JabEncodeResult jab_encode_ex(
    const uint8_t* data, uint64_t data_len,
    uint32_t colors, uint32_t ecc, uint32_t module_px);

extern uint8_t* jab_decode(
    const uint8_t* rgba, uint64_t rgba_len,
    uint32_t width, uint32_t height,
    uint32_t colors, uint32_t ecc, uint32_t module_px,
    uint64_t* out_len);

extern void jab_free(uint8_t* ptr, uint64_t len);
extern void jab_result_free(JabEncodeResult r);
extern const uint8_t* jab_version();
extern int32_t jab_gpu_available();
extern uint64_t jab_capacity(uint32_t colors, uint32_t ecc, uint32_t symbol_side);
*/
import "C"
import (
	"errors"
	"unsafe"
)

// EccLevel controls LDPC error correction strength.
type EccLevel uint32

const (
	EccLow    EccLevel = 0 // rate 3/4 – fastest, least protection
	EccMedium EccLevel = 1 // rate 1/2 – balanced (default)
	EccHigh   EccLevel = 2 // rate 1/3 – strong protection
	EccUltra  EccLevel = 3 // rate 1/4 – maximum protection
)

// Config holds encoding/decoding parameters.
type Config struct {
	Colors   uint32   // 4, 8, 16, 32, 64, 128, or 256
	Ecc      EccLevel // error correction level
	ModulePx uint32   // pixels per module (≥1, default 4)
}

func DefaultConfig() Config {
	return Config{Colors: 8, Ecc: EccMedium, ModulePx: 4}
}

// EncodeResult holds the RGBA image and dimensions.
type EncodeResult struct {
	RGBA   []byte
	Width  uint32
	Height uint32
}

// Encode encodes arbitrary bytes into a JAB RGBA image.
func Encode(data []byte, cfg Config) (*EncodeResult, error) {
	if len(data) == 0 {
		return nil, errors.New("b: empty input data")
	}
	mod_px := cfg.ModulePx
	if mod_px < 1 {
		mod_px = 4
	}

	var outWidth, outHeight C.uint32_t
	var outLen C.uint64_t

	ptr := C.jab_encode(
		(*C.uint8_t)(unsafe.Pointer(&data[0])),
		C.uint64_t(len(data)),
		C.uint32_t(cfg.Colors),
		C.uint32_t(cfg.Ecc),
		C.uint32_t(mod_px),
		&outWidth,
		&outHeight,
		&outLen,
	)
	if ptr == nil {
		return nil, errors.New("b: encode failed")
	}
	defer C.jab_free(ptr, outLen)

	rgba := C.GoBytes(unsafe.Pointer(ptr), C.int(outLen))
	return &EncodeResult{
		RGBA:   rgba,
		Width:  uint32(outWidth),
		Height: uint32(outHeight),
	}, nil
}

// EncodeEx is the same as Encode but uses the struct-return ABI (fewer CGO calls).
func EncodeEx(data []byte, cfg Config) (*EncodeResult, error) {
	if len(data) == 0 {
		return nil, errors.New("b: empty input data")
	}
	mod_px := cfg.ModulePx
	if mod_px < 1 {
		mod_px = 4
	}
	r := C.jab_encode_ex(
		(*C.uint8_t)(unsafe.Pointer(&data[0])),
		C.uint64_t(len(data)),
		C.uint32_t(cfg.Colors),
		C.uint32_t(cfg.Ecc),
		C.uint32_t(mod_px),
	)
	if r.error != 0 || r.rgba == nil {
		return nil, errors.New("b: encode_ex failed")
	}
	defer C.jab_result_free(r)
	rgba := C.GoBytes(unsafe.Pointer(r.rgba), C.int(r.rgba_len))
	return &EncodeResult{
		RGBA:   rgba,
		Width:  uint32(r.img_width),
		Height: uint32(r.img_height),
	}, nil
}

// Decode recovers the original bytes from a JAB RGBA image.
func Decode(rgba []byte, width, height uint32, cfg Config) ([]byte, error) {
	if len(rgba) == 0 {
		return nil, errors.New("b: empty rgba input")
	}
	mod_px := cfg.ModulePx
	if mod_px < 1 {
		mod_px = 4
	}
	var outLen C.uint64_t
	ptr := C.jab_decode(
		(*C.uint8_t)(unsafe.Pointer(&rgba[0])),
		C.uint64_t(len(rgba)),
		C.uint32_t(width),
		C.uint32_t(height),
		C.uint32_t(cfg.Colors),
		C.uint32_t(cfg.Ecc),
		C.uint32_t(mod_px),
		&outLen,
	)
	if ptr == nil {
		return nil, errors.New("b: decode failed")
	}
	defer C.jab_free(ptr, outLen)
	return C.GoBytes(unsafe.Pointer(ptr), C.int(outLen)), nil
}

// Version returns the b library version string.
func Version() string {
	return C.GoString((*C.char)(unsafe.Pointer(C.jab_version())))
}

// GPUAvailable returns true if GPU acceleration is compiled in and available.
func GPUAvailable() bool {
	return C.jab_gpu_available() == 1
}

// Capacity returns the maximum data bytes encodable in one symbol.
func Capacity(colors uint32, ecc EccLevel, symbolSide uint32) uint64 {
	return uint64(C.jab_capacity(C.uint32_t(colors), C.uint32_t(ecc), C.uint32_t(symbolSide)))
}

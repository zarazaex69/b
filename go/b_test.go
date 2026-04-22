package b_test

import (
	"bytes"
	"testing"

	jab "github.com/zarazaex69/b/go"
)

func TestVersion(t *testing.T) {
	v := jab.Version()
	if v == "" {
		t.Fatal("expected non-empty version")
	}
	t.Logf("b version: %s", v)
}

func TestEncodeDecodeRoundTrip(t *testing.T) {
	payload := []byte("Hello, JAB Code! Real-time color barcode.")
	cfg := jab.Config{Colors: 8, Ecc: jab.EccMedium, ModulePx: 4}

	enc, err := jab.Encode(payload, cfg)
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	t.Logf("encoded: %d×%d px, %d bytes RGBA", enc.Width, enc.Height, len(enc.RGBA))

	dec, err := jab.Decode(enc.RGBA, enc.Width, enc.Height, cfg)
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !bytes.HasPrefix(dec, payload) {
		t.Fatalf("mismatch:\n got  %q\n want %q", dec[:len(payload)], payload)
	}
}

func TestEncode256Colors(t *testing.T) {
	payload := make([]byte, 8192) // 8 KB
	for i := range payload { payload[i] = byte(i) }
	cfg := jab.Config{Colors: 256, Ecc: jab.EccLow, ModulePx: 2}

	enc, err := jab.Encode(payload, cfg)
	if err != nil {
		t.Fatalf("encode 256: %v", err)
	}
	t.Logf("8 KB → %d×%d px image", enc.Width, enc.Height)
}

func TestCapacity(t *testing.T) {
	cap8   := jab.Capacity(8,   jab.EccMedium, 50)
	cap256 := jab.Capacity(256, jab.EccMedium, 50)
	if cap256 <= cap8 {
		t.Errorf("256-color capacity (%d) should exceed 8-color (%d)", cap256, cap8)
	}
	t.Logf("50×50 symbol capacity: 8-color=%d, 256-color=%d bytes", cap8, cap256)
}

func BenchmarkEncode8K(b *testing.B) {
	payload := make([]byte, 8192)
	cfg := jab.Config{Colors: 256, Ecc: jab.EccMedium, ModulePx: 2}
	b.SetBytes(int64(len(payload)))
	b.ResetTimer()
	for range b.N {
		_, err := jab.Encode(payload, cfg)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkEncodeEx8K(b *testing.B) {
	payload := make([]byte, 8192)
	cfg := jab.Config{Colors: 256, Ecc: jab.EccMedium, ModulePx: 2}
	b.SetBytes(int64(len(payload)))
	b.ResetTimer()
	for range b.N {
		_, err := jab.EncodeEx(payload, cfg)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func TestAllColorModes(t *testing.T) {
	payload := []byte("Test payload for color modes")
	colorModes := []uint32{4, 8, 16, 32, 64, 128, 256}

	for _, colors := range colorModes {
		cfg := jab.Config{Colors: colors, Ecc: jab.EccMedium, ModulePx: 4}
		enc, err := jab.Encode(payload, cfg)
		if err != nil {
			t.Errorf("encode with %d colors: %v", colors, err)
			continue
		}
		dec, err := jab.Decode(enc.RGBA, enc.Width, enc.Height, cfg)
		if err != nil {
			t.Errorf("decode with %d colors: %v", colors, err)
			continue
		}
		if !bytes.HasPrefix(dec, payload) {
			t.Errorf("%d colors: mismatch", colors)
		}
		t.Logf("%d colors: %dx%d px", colors, enc.Width, enc.Height)
	}
}

func TestAllEccLevels(t *testing.T) {
	payload := []byte("Test payload for ECC levels")
	eccLevels := []jab.EccLevel{jab.EccLow, jab.EccMedium, jab.EccHigh, jab.EccUltra}
	eccNames := []string{"Low", "Medium", "High", "Ultra"}

	for i, ecc := range eccLevels {
		cfg := jab.Config{Colors: 8, Ecc: ecc, ModulePx: 4}
		enc, err := jab.Encode(payload, cfg)
		if err != nil {
			t.Errorf("encode with ECC %s: %v", eccNames[i], err)
			continue
		}
		dec, err := jab.Decode(enc.RGBA, enc.Width, enc.Height, cfg)
		if err != nil {
			t.Errorf("decode with ECC %s: %v", eccNames[i], err)
			continue
		}
		if !bytes.HasPrefix(dec, payload) {
			t.Errorf("ECC %s: mismatch", eccNames[i])
		}
		t.Logf("ECC %s: %dx%d px", eccNames[i], enc.Width, enc.Height)
	}
}

func TestModuleSizes(t *testing.T) {
	payload := []byte("Module size test")
	moduleSizes := []uint32{1, 2, 4, 8, 16}

	for _, mpx := range moduleSizes {
		cfg := jab.Config{Colors: 8, Ecc: jab.EccMedium, ModulePx: mpx}
		enc, err := jab.Encode(payload, cfg)
		if err != nil {
			t.Errorf("encode with ModulePx=%d: %v", mpx, err)
			continue
		}
		dec, err := jab.Decode(enc.RGBA, enc.Width, enc.Height, cfg)
		if err != nil {
			t.Errorf("decode with ModulePx=%d: %v", mpx, err)
			continue
		}
		if !bytes.HasPrefix(dec, payload) {
			t.Errorf("ModulePx=%d: mismatch", mpx)
		}
		t.Logf("ModulePx=%d: %dx%d px (symbol side = %d modules)", mpx, enc.Width, enc.Height, enc.Width/mpx)
	}
}

func TestLargePayloads(t *testing.T) {
	sizes := []int{500, 1000, 2000, 4000, 8000}

	for _, size := range sizes {
		payload := make([]byte, size)
		for i := range payload {
			payload[i] = byte(i % 256)
		}
		cfg := jab.Config{Colors: 256, Ecc: jab.EccLow, ModulePx: 2}
		enc, err := jab.Encode(payload, cfg)
		if err != nil {
			t.Errorf("encode %d bytes: %v", size, err)
			continue
		}
		dec, err := jab.Decode(enc.RGBA, enc.Width, enc.Height, cfg)
		if err != nil {
			t.Errorf("decode %d bytes: %v", size, err)
			continue
		}
		if len(dec) < size {
			t.Errorf("%d bytes: decoded only %d bytes", size, len(dec))
			continue
		}
		if !bytes.Equal(dec[:size], payload) {
			t.Errorf("%d bytes: mismatch", size)
		}
		t.Logf("%d bytes → %dx%d px", size, enc.Width, enc.Height)
	}
}

func TestGPUAvailable(t *testing.T) {
	available := jab.GPUAvailable()
	t.Logf("GPU available: %v", available)
}

func TestEncodeExVsEncode(t *testing.T) {
	payload := []byte("Compare Encode vs EncodeEx results")
	cfg := jab.Config{Colors: 8, Ecc: jab.EccMedium, ModulePx: 4}

	enc1, err := jab.Encode(payload, cfg)
	if err != nil {
		t.Fatalf("Encode: %v", err)
	}

	enc2, err := jab.EncodeEx(payload, cfg)
	if err != nil {
		t.Fatalf("EncodeEx: %v", err)
	}

	if enc1.Width != enc2.Width || enc1.Height != enc2.Height {
		t.Errorf("dimensions differ: %dx%d vs %dx%d", enc1.Width, enc1.Height, enc2.Width, enc2.Height)
	}
	if !bytes.Equal(enc1.RGBA, enc2.RGBA) {
		t.Error("RGBA data differs")
	}
	t.Log("Encode and EncodeEx produce identical results")
}

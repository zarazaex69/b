package b_test

import (
	"bytes"
	"testing"

	jab "github.com/yourorg/b/go"
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

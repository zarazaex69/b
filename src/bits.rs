/// Bit buffer for packing/unpacking N-bit groups.
#[derive(Default)]
pub struct BitBuf {
    data: Vec<u64>,
    bit_len: usize,
}

impl BitBuf {
    #[inline(always)]
    pub fn with_capacity(bits: usize) -> Self {
        Self {
            data: Vec::with_capacity((bits + 63) >> 6),
            bit_len: 0,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut buf = Self::with_capacity(bytes.len() << 3);
        for &b in bytes {
            buf.push_bits(b as u64, 8);
        }
        buf
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.bit_len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.bit_len == 0
    }

    /// Append `count` low-order bits from `value`.
    #[inline(always)]
    pub fn push_bits(&mut self, value: u64, count: u8) {
        let mask = if count == 64 { u64::MAX } else { (1u64 << count) - 1 };
        let value = value & mask;

        let word_idx  = self.bit_len >> 6;
        let bit_off   = self.bit_len & 63;

        if word_idx >= self.data.len() {
            self.data.push(0);
        }
        if bit_off + count as usize > 64 && word_idx + 1 >= self.data.len() {
            self.data.push(0);
        }

        unsafe {
            *self.data.get_unchecked_mut(word_idx) |= value << bit_off;
        }
        let overflow = (bit_off + count as usize).saturating_sub(64);
        if overflow > 0 {
            unsafe {
                *self.data.get_unchecked_mut(word_idx + 1) |= value >> (count as usize - overflow);
            }
        }
        self.bit_len += count as usize;
    }

    /// Read `count` bits starting at bit position `pos`.
    #[inline(always)]
    pub fn read_bits(&self, pos: usize, count: u8) -> u64 {
        let mask = if count == 64 { u64::MAX } else { (1u64 << count) - 1 };
        let word_idx = pos >> 6;
        let bit_off  = pos & 63;
        let lo = unsafe { *self.data.get_unchecked(word_idx) >> bit_off };
        let need_hi = bit_off + count as usize > 64;
        let hi = if need_hi && word_idx + 1 < self.data.len() {
            unsafe { *self.data.get_unchecked(word_idx + 1) << (64 - bit_off) }
        } else {
            0
        };
        (lo | hi) & mask
    }

    /// Convert to a flat byte vec (zero-padded to byte boundary).
    pub fn to_bytes(&self) -> Vec<u8> {
        let byte_len = (self.bit_len + 7) >> 3;
        let mut out = Vec::with_capacity(byte_len);
        let mut pos = 0;
        while pos < self.bit_len {
            let rem = (self.bit_len - pos).min(8);
            out.push(self.read_bits(pos, rem as u8) as u8);
            pos += 8;
        }
        out
    }

    /// Iterate color indices (each `bpm` bits).
    #[inline(always)]
    pub fn modules(&self, bpm: u8) -> impl Iterator<Item = u8> + '_ {
        let total = self.bit_len / bpm as usize;
        (0..total).map(move |i| self.read_bits(i * bpm as usize, bpm) as u8)
    }

    #[inline(always)]
    pub fn inner(&self) -> &[u64] {
        &self.data
    }

    #[inline(always)]
    pub fn inner_mut(&mut self) -> &mut Vec<u64> {
        &mut self.data
    }

    #[inline(always)]
    pub fn set_bit(&mut self, pos: usize, val: bool) {
        let word  = pos >> 6;
        let shift = pos & 63;
        unsafe {
            let w = self.data.get_unchecked_mut(word);
            if val {
                *w |=  1u64 << shift;
            } else {
                *w &= !(1u64 << shift);
            }
        }
    }

    #[inline(always)]
    pub fn get_bit(&self, pos: usize) -> bool {
        let word  = pos >> 6;
        let shift = pos & 63;
        unsafe { (*self.data.get_unchecked(word) >> shift) & 1 == 1 }
    }

    pub fn resize_bits(&mut self, bits: usize) {
        let words = (bits + 63) >> 6;
        self.data.resize(words, 0);
        self.bit_len = bits;
    }
}

/// XOR two bit buffers of the same length in-place (a ^= b).
#[inline(always)]
pub fn xor_assign(a: &mut BitBuf, b: &BitBuf) {
    for (aw, &bw) in a.data.iter_mut().zip(b.inner()) {
        *aw ^= bw;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let bytes = b"hello world!";
        let buf = BitBuf::from_bytes(bytes);
        assert_eq!(buf.to_bytes(), bytes);
    }

    #[test]
    fn push_read_8() {
        let mut buf = BitBuf::default();
        buf.push_bits(0b10110100, 8);
        buf.push_bits(0b11001010, 8);
        assert_eq!(buf.read_bits(0, 8), 0b10110100);
        assert_eq!(buf.read_bits(8, 8), 0b11001010);
    }

    #[test]
    fn modules_3bpm() {
        let mut buf = BitBuf::default();
        for v in 0u8..8 {
            buf.push_bits(v as u64, 3);
        }
        let mods: Vec<u8> = buf.modules(3).collect();
        assert_eq!(mods, (0u8..8).collect::<Vec<_>>());
    }
}

use crate::config::ColorCount;

/// RGB color triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    #[inline(always)]
    pub fn distance_sq(self, other: Rgb) -> u32 {
        let dr = self.0 as i32 - other.0 as i32;
        let dg = self.1 as i32 - other.1 as i32;
        let db = self.2 as i32 - other.2 as i32;
        (dr * dr + dg * dg + db * db) as u32
    }
}

// Standard JAB color palette – perceptually maximally distinct colors.
// First 8 are the canonical JAB primaries; the rest extend the palette.
static BASE_PALETTE: [Rgb; 8] = [
    Rgb(0,   0,   0  ), // 0  black
    Rgb(0,   255, 255), // 1  cyan
    Rgb(255, 0,   255), // 2  magenta
    Rgb(255, 255, 0  ), // 3  yellow
    Rgb(255, 0,   0  ), // 4  red
    Rgb(0,   255, 0  ), // 5  green
    Rgb(0,   0,   255), // 6  blue
    Rgb(255, 255, 255), // 7  white
];

/// Build a full palette of `n` colors (n = 4, 8, 16, 32, 64, 128, 256).
/// Colors 0..8 are always the canonical JAB primaries.
/// Beyond 8, we pack the RGB cube uniformly.
pub fn build_palette(cc: ColorCount) -> Vec<Rgb> {
    let n = cc.count();
    let mut palette = Vec::with_capacity(n);

    // First 8: canonical
    for &c in BASE_PALETTE.iter().take(n.min(8)) {
        palette.push(c);
    }

    if n <= 8 {
        return palette;
    }

    // Remaining slots: sample the RGB cube at equal intervals.
    let extra = n - 8;
    let steps = (extra as f64).cbrt().ceil() as usize + 1;

    'outer: for r in 0..steps {
        for g in 0..steps {
            for b in 0..steps {
                if palette.len() >= n {
                    break 'outer;
                }
                let rv = (r * 255 / (steps - 1).max(1)) as u8;
                let gv = (g * 255 / (steps - 1).max(1)) as u8;
                let bv = (b * 255 / (steps - 1).max(1)) as u8;
                let candidate = Rgb(rv, gv, bv);
                let dup = palette.iter().any(|&c| c.distance_sq(candidate) < 900);
                if !dup {
                    palette.push(candidate);
                }
            }
        }
    }

    let mut gray: u8 = 32;
    while palette.len() < n {
        palette.push(Rgb(gray, gray, gray));
        gray = gray.wrapping_add(13);
    }

    palette.truncate(n);
    palette
}

/// Nearest palette index for an observed RGB value.
#[inline(always)]
pub fn nearest_color(palette: &[Rgb], obs: Rgb) -> u8 {
    let mut best_idx = 0u8;
    let mut best_dist = u32::MAX;
    let (r, g, b) = (obs.0 as i32, obs.1 as i32, obs.2 as i32);
    for (i, c) in palette.iter().enumerate() {
        let dr = c.0 as i32 - r;
        let dg = c.1 as i32 - g;
        let db = c.2 as i32 - b;
        let d = (dr * dr + dg * dg + db * db) as u32;
        if d < best_dist {
            best_dist = d;
            best_idx = i as u8;
            if d == 0 { break; }
        }
    }
    best_idx
}

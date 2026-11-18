//! Colours as 0xRRGGBB, blending, and RGB565 conversion with dithering.

pub type Rgb = u32;

#[inline]
pub fn mix(a: Rgb, b: Rgb, t: u8) -> Rgb {
    let t = t as u32;
    let ch = |s: u32| (((a >> s) & 0xFF) * (255 - t) + ((b >> s) & 0xFF) * t + 127) / 255;
    ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// Text coverage curve: slightly fuller than linear so small text on a
/// bright screen doesn't look washed out (the usual gamma-ish correction).
/// A table, because `powf` per glyph pixel is slow on the ESP32.
#[inline]
pub fn text_alpha(a: u8) -> u8 {
    TEXT_ALPHA[a as usize]
}

static TEXT_ALPHA: [u8; 256] = {
    // Piecewise-linear fit of 255·(a/255)^0.8 through these points.
    const KNOTS: [(u32, u32); 9] = [(0, 0), (8, 16), (16, 28), (32, 48), (64, 84), (96, 117), (128, 147), (192, 203), (255, 255)];
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut k = 0;
        while k + 1 < KNOTS.len() && KNOTS[k + 1].0 < i as u32 {
            k += 1;
        }
        let (x0, y0) = KNOTS[k];
        let (x1, y1) = if k + 1 < KNOTS.len() { KNOTS[k + 1] } else { KNOTS[k] };
        let y = if x1 == x0 { y0 } else { y0 + (y1 - y0) * (i as u32 - x0) / (x1 - x0) };
        t[i] = if y > 255 { 255 } else { y as u8 };
        i += 1;
    }
    t
};

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// RGB888 → RGB565 with a 4×4 ordered dither (hides 6/5-bit banding).
#[inline]
pub fn to565_dithered(c: Rgb, x: i32, y: i32) -> u16 {
    // Quantize on the panel's own scale (a 5-bit level means level×255/31),
    // adding a threshold in [0,1) so the 4×4 tile averages to the true value.
    let d = BAYER[(y & 3) as usize][(x & 3) as usize] as u32;
    let q = |v: u32, max: u32| -> u32 { ((v * max * 16 + (2 * d + 1) * 255 / 2) / (255 * 16)).min(max) };
    let (r, g, b) = (q((c >> 16) & 0xFF, 31), q((c >> 8) & 0xFF, 63), q(c & 0xFF, 31));
    (r << 11 | g << 5 | b) as u16
}

pub fn from565(p: u16) -> Rgb {
    let r = ((p >> 11) & 0x1F) as u32;
    let g = ((p >> 5) & 0x3F) as u32;
    let b = (p & 0x1F) as u32;
    (r << 3 | r >> 2) << 16 | (g << 2 | g >> 4) << 8 | (b << 3 | b >> 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_table_tracks_the_curve() {
        for a in 0..=255u32 {
            let want = (libm::powf(a as f32 / 255.0, 0.8) * 255.0).round() as i32;
            assert!((text_alpha(a as u8) as i32 - want).abs() <= 3, "{a}: {} vs {want}", text_alpha(a as u8));
        }
    }

    #[test]
    fn mix_endpoints() {
        assert_eq!(mix(0x000000, 0xFFFFFF, 0), 0x000000);
        assert_eq!(mix(0x000000, 0xFFFFFF, 255), 0xFFFFFF);
    }

    #[test]
    fn dither_keeps_pure_colours_exact() {
        for (x, y) in [(0, 0), (1, 2), (3, 3)] {
            assert_eq!(to565_dithered(0xFFFFFF, x, y), 0xFFFF);
            assert_eq!(to565_dithered(0x000000, x, y), 0x0000);
        }
    }

    #[test]
    fn dither_averages_to_the_true_colour() {
        // A mid grey that 565 can't hit exactly averages out over the 4×4 tile.
        let c = 0x7B7B7B;
        let mut sum = 0u32;
        for y in 0..4 {
            for x in 0..4 {
                sum += (from565(to565_dithered(c, x, y)) >> 16) & 0xFF;
            }
        }
        let avg = sum / 16;
        assert!(avg.abs_diff(0x7B) <= 2, "avg {avg:x}");
    }
}

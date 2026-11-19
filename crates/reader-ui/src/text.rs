//! Anti-aliased TrueType text (fontdue), with a glyph cache.
//!
//! Fonts are subset to Latin-1 + typographic punctuation by
//! `tools/make_fonts.py` and embedded in flash (≈1.1 MB for all of them).
//! Each face is parsed on first use and glyphs are rasterized once per size.

#[allow(unused_imports)]
use core_maths::CoreFloat;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use ab_glyph::{point, Font as _, FontRef, PxScale};
use alloc::vec;
use reader_core::Style;

use crate::canvas::Canvas;
use crate::color::Rgb;

macro_rules! faces {
    ($($name:literal),* $(,)?) => {
        static FACE_DATA: &[&[u8]] = &[$(include_bytes!(concat!("../../../assets/fonts/", $name, ".ttf"))),*];
    };
}

faces!(
    "atkinson-r", "atkinson-b", "atkinson-i", "atkinson-bi",
    "literata-r", "literata-b", "literata-i", "literata-bi",
    "plexsans-r", "plexsans-b", "plexsans-i", "plexsans-bi",
    "plexserif-r", "plexserif-b", "plexserif-i", "plexserif-bi",
    "crimson-r", "crimson-b", "crimson-i", "crimson-bi",
    "courier-r", "courier-b", "courier-i", "courier-bi",
    "gelasio-r", "gelasio-b", "gelasio-i", "gelasio-bi",
    "plexmono-r", "plexmono-b",
    "selawik-r", "selawik-b",
);

/// Index into the embedded faces.
pub type Face = u8;

pub const PLEX_SANS: Face = 8;
pub const PLEX_SANS_BOLD: Face = 9;
pub const MONO: Face = 28;
pub const MONO_BOLD: Face = 29;
pub const SELAWIK: Face = 30;
pub const SELAWIK_BOLD: Face = 31;

/// A reading family: regular, bold, italic, bold italic.
pub struct Family {
    pub name: &'static str,
    faces: [Face; 4],
}

impl Family {
    pub fn face(&self, style: Style) -> Face {
        self.faces[style.bold() as usize | (style.italic() as usize) << 1]
    }
}

pub static FAMILIES: [Family; 7] = [
    Family { name: "Atkinson Hyperlegible", faces: [0, 1, 2, 3] },
    Family { name: "Literata", faces: [4, 5, 6, 7] },
    Family { name: "Plex Sans", faces: [8, 9, 10, 11] },
    Family { name: "Plex Serif", faces: [12, 13, 14, 15] },
    Family { name: "Crimson", faces: [16, 17, 18, 19] },
    Family { name: "Courier Prime", faces: [20, 21, 22, 23] },
    // The original e-reader's speed-reading face.
    Family { name: "Gelasio", faces: [24, 25, 26, 27] },
];

pub static FONT_NAMES: [&str; 7] = [
    FAMILIES[0].name,
    FAMILIES[1].name,
    FAMILIES[2].name,
    FAMILIES[3].name,
    FAMILIES[4].name,
    FAMILIES[5].name,
    FAMILIES[6].name,
];

pub fn family(i: u8) -> &'static Family {
    &FAMILIES[i as usize % FAMILIES.len()]
}

struct Glyph {
    /// Offset of the bitmap's top-left from the pen position on the baseline.
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    bitmap: Vec<u8>,
}

/// Cap on cached glyph bitmaps before the cache is flushed (bounds RAM).
const CACHE_MAX: usize = 400;

/// Fonts are read straight from flash (`ab_glyph` parses lazily and keeps
/// no per-glyph tables), so 28 faces cost almost no RAM; only the glyph
/// bitmaps in use are cached.
pub struct Fonts {
    parsed: Vec<Option<FontRef<'static>>>,
    glyphs: BTreeMap<(Face, char, u16), Glyph>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub height: f32,
}

impl Fonts {
    pub fn new() -> Self {
        Fonts { parsed: (0..FACE_DATA.len()).map(|_| None).collect(), glyphs: BTreeMap::new() }
    }

    fn font(&mut self, face: Face) -> &FontRef<'static> {
        self.parsed[face as usize].get_or_insert_with(|| FontRef::try_from_slice(FACE_DATA[face as usize]).expect("embedded font"))
    }

    /// Pixels per font unit at an em size of `px`.
    fn unit(&mut self, face: Face, px: f32) -> f32 {
        px / self.font(face).units_per_em().unwrap_or(1000.0)
    }

    pub fn line(&mut self, face: Face, px: f32) -> LineMetrics {
        let k = self.unit(face, px);
        let f = self.font(face);
        let (a, d, g) = (f.ascent_unscaled(), f.descent_unscaled(), f.line_gap_unscaled());
        LineMetrics { ascent: a * k, descent: -d * k, height: (a - d + g) * k }
    }

    fn advance(&mut self, face: Face, c: char, px: f32) -> f32 {
        let k = self.unit(face, px);
        let f = self.font(face);
        f.h_advance_unscaled(f.glyph_id(c)) * k
    }

    fn kern(&mut self, face: Face, a: char, b: char, px: f32) -> f32 {
        let k = self.unit(face, px);
        let f = self.font(face);
        f.kern_unscaled(f.glyph_id(a), f.glyph_id(b)) * k
    }

    /// Width of `s` in pixels.
    pub fn measure(&mut self, face: Face, px: f32, s: &str) -> f32 {
        let mut w = 0.0;
        let mut prev = None;
        for c in s.chars() {
            if let Some(p) = prev {
                w += self.kern(face, p, c, px);
            }
            w += self.advance(face, c, px);
            prev = Some(c);
        }
        w
    }

    fn rasterize(&mut self, face: Face, c: char, px: f32) -> Glyph {
        let f = self.font(face);
        let upem = f.units_per_em().unwrap_or(1000.0);
        // ab_glyph scales by line height; convert our em size to that.
        let scale = PxScale::from(px * f.height_unscaled() / upem);
        let g = f.glyph_id(c).with_scale_and_position(scale, point(0.0, 0.0));
        match f.outline_glyph(g) {
            Some(og) => {
                let b = og.px_bounds();
                let (w, h) = (b.width() as usize, b.height() as usize);
                let mut bitmap = vec![0u8; w * h];
                og.draw(|x, y, cov| {
                    if (x as usize) < w && (y as usize) < h {
                        bitmap[y as usize * w + x as usize] = (cov.min(1.0) * 255.0) as u8;
                    }
                });
                Glyph { x: b.min.x as i32, y: b.min.y as i32, w, h, bitmap }
            }
            None => Glyph { x: 0, y: 0, w: 0, h: 0, bitmap: Vec::new() },
        }
    }

    /// Draws `s` with its baseline at `y`; returns the advance.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, cv: &mut Canvas, face: Face, px: f32, s: &str, x: f32, y: f32, color: Rgb) -> f32 {
        let size = (px * 4.0) as u16;
        let mut pen = x;
        let mut prev = None;
        let base = y.round() as i32;
        for c in s.chars() {
            if let Some(p) = prev {
                pen += self.kern(face, p, c, px);
            }
            if self.glyphs.len() > CACHE_MAX {
                self.glyphs.clear();
            }
            if !self.glyphs.contains_key(&(face, c, size)) {
                let g = self.rasterize(face, c, px);
                self.glyphs.insert((face, c, size), g);
            }
            let g = &self.glyphs[&(face, c, size)];
            cv.coverage(pen.round() as i32 + g.x, base + g.y, g.w, g.h, &g.bitmap, color);
            pen += self.advance(face, c, px);
            prev = Some(c);
        }
        pen - x
    }

    /// Draws `s`, shortening it with "…" if it is wider than `max_w`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_fit(&mut self, cv: &mut Canvas, face: Face, px: f32, s: &str, x: f32, y: f32, max_w: f32, color: Rgb) -> f32 {
        if self.measure(face, px, s) <= max_w {
            return self.draw(cv, face, px, s, x, y, color);
        }
        let ell = self.measure(face, px, "…");
        let mut end = 0;
        let mut w = 0.0;
        for (i, c) in s.char_indices() {
            let a = self.advance(face, c, px);
            if w + a + ell > max_w {
                break;
            }
            w += a;
            end = i + c.len_utf8();
        }
        let shown = s[..end].trim_end();
        let adv = self.draw(cv, face, px, shown, x, y, color);
        adv + self.draw(cv, face, px, "…", x + adv, y, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::ToString;

    #[test]
    fn every_face_parses_and_has_latin1() {
        let mut f = Fonts::new();
        for face in 0..FACE_DATA.len() as Face {
            for c in ['a', 'Z', 'é', 'ß', '“', '—', '…'] {
                assert!(f.measure(face, 20.0, &c.to_string()) > 0.0, "face {face} lacks {c:?}");
            }
        }
    }

    #[test]
    fn text_is_anti_aliased() {
        let mut f = Fonts::new();
        let mut cv = Canvas::new(120, 40);
        cv.fill(cv.bounds(), 0xFFFFFF);
        f.draw(&mut cv, 0, 28.0, "Word", 4.0, 30.0, 0x000000);
        let mut greys = 0;
        for y in 0..40 {
            for x in 0..120 {
                let p = cv.pixel(x, y) & 0xFF;
                if p > 0x10 && p < 0xF0 {
                    greys += 1;
                }
            }
        }
        assert!(greys > 30, "only {greys} intermediate pixels");
    }

    #[test]
    fn fit_adds_ellipsis() {
        let mut f = Fonts::new();
        let mut cv = Canvas::new(200, 30);
        let full = f.measure(PLEX_SANS, 14.0, "A rather long chapter title here");
        let w = f.draw_fit(&mut cv, PLEX_SANS, 14.0, "A rather long chapter title here", 0.0, 20.0, 100.0, 0xFFFFFF);
        assert!(w <= 100.5 && w < full);
    }
}

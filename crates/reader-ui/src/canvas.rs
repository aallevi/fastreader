//! A 24-bit RGB canvas with anti-aliased shapes.
//!
//! Everything is drawn in full colour and blended with coverage, then
//! converted to the panel's RGB565 with ordered dithering, so gradients don't
//! band and edges and text stay smooth. 320×240 is 300 KB (in PSRAM).

#[allow(unused_imports)]
use core_maths::CoreFloat;
use alloc::vec;
use alloc::vec::Vec;

use crate::color::{mix, to565_dithered, Rgb};
use crate::geom::Rect;

/// A vertical gradient: stops are (position 0–255 over the rect height, colour).
#[derive(Clone, Copy)]
pub enum Paint<'a> {
    Solid(Rgb),
    Vertical(&'a [(u8, Rgb)]),
    /// Left-to-right gradient (Windows 98 title bars). Only for `paint_rect`.
    Horizontal(&'a [(u8, Rgb)]),
}

impl Paint<'_> {
    fn at(&self, t: f32) -> Rgb {
        match self {
            Paint::Solid(c) => *c,
            Paint::Vertical(stops) | Paint::Horizontal(stops) => gradient_at(stops, t),
        }
    }
}

pub fn gradient_at(stops: &[(u8, Rgb)], t: f32) -> Rgb {
    let p = (t.clamp(0.0, 1.0) * 255.0) as u8;
    let mut prev = stops[0];
    for &s in stops {
        if p <= s.0 {
            if s.0 == prev.0 {
                return s.1;
            }
            let f = (p - prev.0) as u32 * 255 / (s.0 - prev.0) as u32;
            return mix(prev.1, s.1, f as u8);
        }
        prev = s;
    }
    prev.1
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    px: Vec<Rgb>,
    clip: Rect,
    /// Pixels touched since last reset (efficiency checks).
    pub writes: u64,
    /// Draw text coverage linearly (dark themes) instead of slightly boosted.
    pub linear_text: bool,
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Canvas { w: w as i32, h: h as i32, px: vec![0; (w * h) as usize], clip: Rect::new(0, 0, w as i32, h as i32), writes: 0, linear_text: false }
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }

    pub fn pixel(&self, x: i32, y: i32) -> Rgb {
        self.px[(y * self.w + x) as usize]
    }

    /// Restricts drawing to `r` until [`Canvas::unclip`].
    pub fn clip(&mut self, r: Rect) {
        self.clip = r.intersect(&self.bounds());
    }

    pub fn unclip(&mut self) {
        self.clip = self.bounds();
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, c: Rgb, a: u8) {
        if a == 0 || !self.clip.contains(x, y) {
            return;
        }
        let i = (y * self.w + x) as usize;
        self.px[i] = if a == 255 { c } else { mix(self.px[i], c, a) };
        self.writes += 1;
    }

    pub fn fill(&mut self, r: Rect, c: Rgb) {
        let r = r.intersect(&self.clip);
        for y in r.y..r.bottom() {
            let s = (y * self.w + r.x) as usize;
            self.px[s..s + r.w as usize].fill(c);
        }
        self.writes += (r.w * r.h).max(0) as u64;
    }

    pub fn paint_rect(&mut self, r: Rect, p: Paint) {
        match p {
            Paint::Solid(c) => self.fill(r, c),
            Paint::Vertical(_) => {
                for y in r.y..r.bottom() {
                    let t = (y - r.y) as f32 / (r.h - 1).max(1) as f32;
                    self.fill(Rect::new(r.x, y, r.w, 1), p.at(t));
                }
            }
            Paint::Horizontal(_) => {
                for x in r.x..r.right() {
                    let t = (x - r.x) as f32 / (r.w - 1).max(1) as f32;
                    self.fill(Rect::new(x, r.y, 1, r.h), p.at(t));
                }
            }
        }
    }

    /// Fills a shape given by a signed distance function (negative inside),
    /// anti-aliased over one pixel. `paint` is sampled over `r`'s height.
    fn sdf_fill(&mut self, r: Rect, p: Paint, alpha: u8, sdf: impl Fn(f32, f32) -> f32) {
        let b = Rect::new(r.x - 1, r.y - 1, r.w + 2, r.h + 2).intersect(&self.clip);
        for y in b.y..b.bottom() {
            let c = p.at((y - r.y) as f32 / (r.h - 1).max(1) as f32);
            for x in b.x..b.right() {
                let d = sdf(x as f32 + 0.5, y as f32 + 0.5);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.put(x, y, c, (cov * alpha as f32) as u8);
                }
            }
        }
    }

    pub fn rrect(&mut self, r: Rect, radius: f32, p: Paint) {
        self.rrect_alpha(r, radius, p, 255);
    }

    pub fn rrect_alpha(&mut self, r: Rect, radius: f32, p: Paint, alpha: u8) {
        if r.is_empty() {
            return;
        }
        let rad = radius.min(r.w as f32 / 2.0).min(r.h as f32 / 2.0);
        // Rows clear of the corners are plain spans: fill them directly.
        let inner = Rect::new(r.x, r.y + rad.ceil() as i32, r.w, r.h - 2 * rad.ceil() as i32);
        if alpha == 255 && !inner.is_empty() {
            for y in inner.y..inner.bottom() {
                let c = p.at((y - r.y) as f32 / (r.h - 1).max(1) as f32);
                self.fill(Rect::new(r.x, y, r.w, 1), c);
            }
            let top = Rect::new(r.x, r.y, r.w, inner.y - r.y);
            let bot = Rect::new(r.x, inner.bottom(), r.w, r.bottom() - inner.bottom());
            for part in [top, bot] {
                let saved = self.clip;
                self.clip = self.clip.intersect(&part);
                self.sdf_fill(r, p, alpha, |x, y| sd_rrect(x, y, r, rad));
                self.clip = saved;
            }
        } else {
            self.sdf_fill(r, p, alpha, |x, y| sd_rrect(x, y, r, rad));
        }
    }

    /// A 1 px anti-aliased outline just inside `r`.
    pub fn rrect_stroke(&mut self, r: Rect, radius: f32, c: Rgb) {
        let rad = radius.min(r.w as f32 / 2.0).min(r.h as f32 / 2.0);
        self.sdf_fill(r, Paint::Solid(c), 255, |x, y| (sd_rrect(x, y, r, rad) + 0.5).abs() - 0.5);
    }

    pub fn circle(&mut self, cx: f32, cy: f32, rad: f32, p: Paint) {
        let r = Rect::new((cx - rad).floor() as i32, (cy - rad).floor() as i32, (rad * 2.0).ceil() as i32 + 1, (rad * 2.0).ceil() as i32 + 1);
        self.sdf_fill(r, p, 255, |x, y| libm::hypotf(x - cx, y - cy) - rad);
    }

    pub fn ring(&mut self, cx: f32, cy: f32, rad: f32, width: f32, c: Rgb) {
        let r = Rect::new((cx - rad).floor() as i32, (cy - rad).floor() as i32, (rad * 2.0).ceil() as i32 + 1, (rad * 2.0).ceil() as i32 + 1);
        self.sdf_fill(r, Paint::Solid(c), 255, |x, y| (libm::hypotf(x - cx, y - cy) - rad + width / 2.0).abs() - width / 2.0);
    }

    /// Soft radial light (Aero glows, orb highlights): alpha falls off to 0 at the edge.
    pub fn glow(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb, max_alpha: u8) {
        let b = Rect::new((cx - rx) as i32, (cy - ry) as i32, (2.0 * rx) as i32 + 1, (2.0 * ry) as i32 + 1).intersect(&self.clip);
        for y in b.y..b.bottom() {
            for x in b.x..b.right() {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let d = dx * dx + dy * dy;
                if d < 1.0 {
                    let f = 1.0 - d;
                    self.put(x, y, c, (f * f * max_alpha as f32) as u8);
                }
            }
        }
    }

    /// A convex polygon (icons), anti-aliased.
    pub fn polygon(&mut self, pts: &[(f32, f32)], c: Rgb) {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        // Signed area tells the winding, so edges' normals point outwards.
        let mut area = 0.0;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            area += a.0 * b.1 - b.0 * a.1;
        }
        let sign = if area > 0.0 { 1.0 } else { -1.0 };
        let r = Rect::new(x0.floor() as i32, y0.floor() as i32, (x1 - x0).ceil() as i32 + 1, (y1 - y0).ceil() as i32 + 1);
        self.sdf_fill(r, Paint::Solid(c), 255, |x, y| {
            let mut d = f32::MIN;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                let len = libm::hypotf(ex, ey).max(1e-6);
                // Distance to the edge line, positive outside.
                let dist = sign * ((x - a.0) * ey - (y - a.1) * ex) / len;
                d = d.max(dist);
            }
            d
        });
    }

    /// A thick line segment with round-ish ends (as a rotated rectangle).
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: Rgb) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = libm::hypotf(dx, dy).max(1e-6);
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let (ex, ey) = (dx / len * width / 4.0, dy / len * width / 4.0);
        self.polygon(&[
            (a.0 + nx - ex, a.1 + ny - ey),
            (b.0 + nx + ex, b.1 + ny + ey),
            (b.0 - nx + ex, b.1 - ny + ey),
            (a.0 - nx - ex, a.1 - ny - ey),
        ], c);
    }

    /// Blends an 8-bit coverage bitmap (a glyph) in colour `c`.
    pub fn coverage(&mut self, x: i32, y: i32, w: usize, h: usize, cov: &[u8], c: Rgb) {
        for row in 0..h {
            for col in 0..w {
                let a = cov[row * w + col];
                if a != 0 {
                    let a = if self.linear_text { a } else { crate::color::text_alpha(a) };
                    self.put(x + col as i32, y + row as i32, c, a);
                }
            }
        }
    }

    /// Pixels of `r` in RGB565 (big-endian order is up to the display),
    /// with 4×4 ordered dithering to hide gradient banding.
    pub fn rgb565(&self, r: Rect) -> impl Iterator<Item = u16> + '_ {
        let r = r.intersect(&self.bounds());
        (r.y..r.bottom()).flat_map(move |y| (r.x..r.right()).map(move |x| to565_dithered(self.pixel(x, y), x, y)))
    }
}

fn sd_rrect(x: f32, y: f32, r: Rect, rad: f32) -> f32 {
    let (hw, hh) = (r.w as f32 / 2.0, r.h as f32 / 2.0);
    let qx = (x - (r.x as f32 + hw)).abs() - (hw - rad);
    let qy = (y - (r.y as f32 + hh)).abs() - (hh - rad);
    libm::hypotf(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - rad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rect_fills_inside_and_softens_corners() {
        let mut c = Canvas::new(40, 40);
        c.rrect(Rect::new(5, 5, 30, 30), 8.0, Paint::Solid(0xFFFFFF));
        assert_eq!(c.pixel(20, 20), 0xFFFFFF);
        assert_eq!(c.pixel(5, 20), 0xFFFFFF);
        assert_eq!(c.pixel(5, 5), 0, "corner is cut");
        let edge = c.pixel(7, 7);
        assert!(edge != 0 && edge != 0xFFFFFF, "corner edge is anti-aliased: {edge:06x}");
    }

    #[test]
    fn clip_is_respected() {
        let mut c = Canvas::new(20, 20);
        c.clip(Rect::new(0, 0, 10, 20));
        c.circle(10.0, 10.0, 8.0, Paint::Solid(0xFF0000));
        c.unclip();
        assert_eq!(c.pixel(15, 10), 0);
        assert_eq!(c.pixel(5, 10), 0xFF0000);
    }

    #[test]
    fn gradient_hits_its_stops() {
        let stops = [(0u8, 0x000000), (128, 0x808080), (255, 0xFFFFFF)];
        assert_eq!(gradient_at(&stops, 0.0), 0x000000);
        assert_eq!(gradient_at(&stops, 1.0), 0xFFFFFF);
        let mid = gradient_at(&stops, 0.5) >> 16;
        assert!((0x7E..=0x82).contains(&mid));
    }

    #[test]
    fn polygon_winding_does_not_matter() {
        for pts in [[(2.0, 2.0), (18.0, 10.0), (2.0, 18.0)], [(2.0, 2.0), (2.0, 18.0), (18.0, 10.0)]] {
            let mut c = Canvas::new(20, 20);
            c.polygon(&pts, 0xFFFFFF);
            assert_eq!(c.pixel(6, 10), 0xFFFFFF);
            assert_eq!(c.pixel(17, 3), 0);
        }
    }
}

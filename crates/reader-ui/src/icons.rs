//! Vector icons, drawn anti-aliased at any size. `s` is the icon box size.

#[allow(unused_imports)]
use core_maths::CoreFloat;
use crate::canvas::{Canvas, Paint};
use crate::color::Rgb;
use crate::layout::Icon;

pub fn draw(cv: &mut Canvas, icon: Icon, cx: f32, cy: f32, s: f32, c: Rgb) {
    let u = s / 16.0; // design grid: 16 units
    let p = |x: f32, y: f32| (cx + x * u, cy + y * u);
    let tri_r = |cv: &mut Canvas, ox: f32, w: f32, h: f32| cv.polygon(&[p(ox - w / 2.0, -h / 2.0), p(ox + w / 2.0, 0.0), p(ox - w / 2.0, h / 2.0)], c);
    let tri_l = |cv: &mut Canvas, ox: f32, w: f32, h: f32| cv.polygon(&[p(ox + w / 2.0, -h / 2.0), p(ox - w / 2.0, 0.0), p(ox + w / 2.0, h / 2.0)], c);
    let bar = |cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32| {
        let (x0, y0) = p(x, y);
        cv.rrect(crate::geom::Rect::new(x0.round() as i32, y0.round() as i32, (w * u).round().max(1.0) as i32, (h * u).round().max(1.0) as i32), u, Paint::Solid(c));
    };
    match icon {
        Icon::Play => tri_r(cv, 1.0, 9.0, 11.0),
        Icon::Pause => {
            bar(cv, -4.0, -5.5, 3.0, 11.0);
            bar(cv, 1.0, -5.5, 3.0, 11.0);
        }
        Icon::SentenceBack => {
            bar(cv, -6.0, -5.0, 2.0, 10.0);
            tri_l(cv, -0.5, 6.0, 10.0);
            tri_l(cv, 4.5, 6.0, 10.0);
        }
        Icon::SentenceForward => {
            tri_r(cv, -4.5, 6.0, 10.0);
            tri_r(cv, 0.5, 6.0, 10.0);
            bar(cv, 4.0, -5.0, 2.0, 10.0);
        }
        Icon::PageBack => tri_l(cv, -0.5, 7.0, 11.0),
        Icon::PageForward => tri_r(cv, 0.5, 7.0, 11.0),
        Icon::Minus => bar(cv, -5.0, -1.0, 10.0, 2.2),
        Icon::Plus => {
            bar(cv, -5.0, -1.1, 10.0, 2.2);
            bar(cv, -1.1, -5.0, 2.2, 10.0);
        }
        Icon::Menu => {
            for y in [-4.5, -1.0, 2.5] {
                bar(cv, -5.5, y, 11.0, 2.0);
            }
        }
        Icon::Gear => {
            // Eight teeth around a ring.
            for k in 0..8 {
                let a = k as f32 * core::f32::consts::PI / 4.0;
                let (s, co) = (a.sin(), a.cos());
                cv.line(p(co * 3.5, s * 3.5), p(co * 6.6, s * 6.6), 2.6 * u, c);
            }
            cv.ring(cx, cy, 4.4 * u, 2.4 * u, c);
        }
        Icon::Book => {
            // An open book: two pages meeting at the spine.
            cv.polygon(&[p(-7.0, -4.5), p(-0.6, -3.2), p(-0.6, 5.5), p(-7.0, 4.2)], c);
            cv.polygon(&[p(0.6, -3.2), p(7.0, -4.5), p(7.0, 4.2), p(0.6, 5.5)], c);
        }
        Icon::Library => {
            // Books on a shelf, the last one leaning.
            bar(cv, -7.0, -5.5, 3.0, 11.0);
            bar(cv, -3.2, -3.5, 3.0, 9.0);
            bar(cv, 0.6, -5.0, 2.4, 10.5);
            cv.polygon(&[p(3.6, 5.5), p(6.0, -3.8), p(8.2, -3.2), p(5.8, 5.5)], c);
        }
        Icon::Text => {
            // "Aa": a big A and a small a.
            cv.line(p(-7.0, 5.5), p(-3.5, -5.5), 2.0 * u, c);
            cv.line(p(-3.5, -5.5), p(0.0, 5.5), 2.0 * u, c);
            cv.line(p(-5.4, 1.6), p(-1.6, 1.6), 1.8 * u, c);
            cv.ring(cx + 4.2 * u, cy + 2.6 * u, 2.4 * u, 1.7 * u, c);
            cv.line(p(6.9, -0.6), p(6.9, 5.5), 1.8 * u, c);
        }
        Icon::Bolt => {
            // Two overlapping triangles make the zig-zag (polygons must be convex).
            cv.polygon(&[p(2.5, -7.5), p(-4.5, 1.2), p(1.2, 1.2)], c);
            cv.polygon(&[p(-2.5, 7.5), p(4.5, -1.2), p(-1.2, -1.2)], c);
        }
        Icon::Back => {
            cv.line(p(2.5, -5.5), p(-3.0, 0.0), 2.2 * u, c);
            cv.line(p(-3.0, 0.0), p(2.5, 5.5), 2.2 * u, c);
        }
        Icon::Close => {
            cv.line(p(-4.5, -4.5), p(4.5, 4.5), 2.0 * u, c);
            cv.line(p(-4.5, 4.5), p(4.5, -4.5), 2.0 * u, c);
        }
        Icon::Up => cv.polygon(&[p(-5.5, 3.0), p(0.0, -3.5), p(5.5, 3.0)], c),
        Icon::Down => cv.polygon(&[p(-5.5, -3.0), p(0.0, 3.5), p(5.5, -3.0)], c),
    }
}

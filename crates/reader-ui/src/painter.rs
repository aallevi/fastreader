//! Draws the app with a theme, redrawing only what changed.
//!
//! Each frame builds a [`View`] snapshot per screen region and compares it
//! with the previous one. A word change repaints only the old and new word
//! boxes, so the SPI bus to the panel stays mostly idle while reading.
//!
//! Rules every theme follows: the word is centred, with nothing around it;
//! messages replace the title for a moment; info text appears only when
//! paused; buttons are rectangles (rounded, or bevelled in Windows 98);
//! nothing slides except text in the hold-to-scroll line view.

#[allow(unused_imports)]
use core_maths::CoreFloat;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use reader_core::app::{Battery, Feedback, Msg, Screen, Tone};
use reader_core::book::Pause;
use reader_core::settings::{Mode, Skip};
use reader_core::{App, Hit, Settings, Style};

use crate::canvas::{Canvas, Paint};
use crate::color::{mix, Rgb};
use crate::geom::Rect;
use crate::icons;
use crate::layout::{self, highlight_button, hit_test, list_geom, menu_button, toolbar, Button, Icon, Layout, RowKind};
use crate::text::{family, Face, Fonts};
use crate::theme::{ButtonStyle, Chrome, Theme};
use crate::themes::theme;

const WHITE: Rgb = 0xFFFFFF;
const BLACK: Rgb = 0x000000;
const W98_FACE: Rgb = 0xC0C0C0;

/// Reading word sizes (Small / Normal / Large) at 240 px height.
const WORD_PX: [f32; 3] = [30.0, 40.0, 52.0];
/// Page view text sizes.
const PAGE_PX: [f32; 3] = [14.0, 17.0, 21.0];

#[derive(Clone, PartialEq, Debug)]
enum Frame {
    Reading,
    Page,
    Overlay(Screen, Option<Hit>, Option<u32>, Settings, u16, u8),
}

/// Page number and, once known, the page count.
type PageInfo = Option<(usize, Option<usize>)>;

#[derive(Clone, PartialEq, Debug)]
struct View {
    look: Settings,
    frame: Frame,
    header: (String, Option<Battery>, Option<u32>, bool),
    word: (usize, u8, bool, u16),
    context: Option<usize>,
    status: (i32, u64, Option<usize>, Option<usize>, bool),
    toolbar: (bool, Option<Hit>, u16, PageInfo),
}

/// Everything that changes where pages break.
type PagesKey = (u8, u8, usize, i32, i32, u8, (u8, bool, bool, u8));

pub struct Ui {
    pub layout: Layout,
    pub fonts: Fonts,
    last: Option<View>,
    pages_key: Option<PagesKey>,
    /// Bounding box of the word on screen, so the next word only clears that.
    word_box: Rect,
    /// The page on screen: first word, highlighted phrase, highlight shown.
    page_drawn: Option<(usize, core::ops::Range<usize>, bool)>,
}

impl Ui {
    pub fn new(w: u32, h: u32) -> Self {
        Ui { layout: Layout::new(w, h), fonts: Fonts::new(), last: None, pages_key: None, word_box: Rect::default(), page_drawn: None }
    }

    /// Forces a full repaint next time (after wake-up, rotation...).
    pub fn invalidate(&mut self) {
        self.last = None;
        self.page_drawn = None;
    }

    /// The touch target under a point, in screen coordinates.
    pub fn hit(&self, app: &App, x: i32, y: i32) -> Option<Hit> {
        hit_test(app, &self.layout, x, y)
    }

    /// Paints what changed and returns the rectangles to send to the panel.
    pub fn paint(&mut self, app: &mut App, now: u64, cv: &mut Canvas) -> Vec<Rect> {
        if app.screen == Screen::Page || app.settings.mode() == Mode::Page {
            self.paginate(app);
            // Past the measured pages: lay out the page on screen to know where it ends.
            app.page_next = None;
            if app.page_index().is_none() && app.screen == Screen::Page {
                let th = theme(app.settings.theme);
                let start = page_start(app);
                let (_, next) = layout_page(&mut self.fonts, app, page_area(th, &self.layout), start, &self.layout);
                app.page_next = Some(next as u32);
            }
        }
        let th = theme(app.settings.theme);
        cv.linear_text = th.light_on_dark;
        let l = self.layout;
        let lit = app.lit(now);
        let frame = match app.screen {
            Screen::Reading | Screen::Line { .. } => Frame::Reading,
            Screen::Page => Frame::Page,
            s => Frame::Overlay(s, lit, app.toast().map(|t| t.serial), app.settings, app.reader.wpm(), app.reader.chunk()),
        };
        let paused = !app.reader.is_playing();
        let mut look = app.settings;
        look.pos = 0;
        look.wpm = 0;
        let view = View {
            look,
            frame,
            header: (header_title(app), app.battery, app.toast().map(|t| t.serial), lit == Some(Hit::Key(reader_core::Key::Menu))),
            word: (
                app.reader.pos(),
                app.reader.chunk(),
                matches!(app.screen, Screen::Line { .. }),
                // While the ticker glides, every frame is a new picture.
                if app.animating() { (app.reader.progress(now) * 256.0) as u16 } else { 0 },
            ),
            context: (paused || matches!(app.screen, Screen::Line { .. })).then_some(app.reader.pos()),
            status: (progress_px(app, &l), app.reader.ms_left() / 60_000, app.page_index(), app.reader.book.chapter_index(app.reader.pos()), paused),
            toolbar: (app.reader.is_playing(), lit, app.reader.wpm(), app.page_index().map(|i| (i, app.pages_done.then_some(app.pages.len())))),
        };
        let old = self.last.take();
        let full = old.as_ref().is_none_or(|o| o.look != view.look || o.frame != view.frame);
        let f = &mut self.fonts;
        let word_box = &mut self.word_box;
        let mut damage = Vec::new();
        let ctx = Ctx { th, l: &l, app, now };

        if app.screen.is_overlay() {
            if full {
                draw_list(f, cv, &ctx);
                damage.push(l.screen);
            }
        } else if full {
            cv.paint_rect(l.screen, Paint::Vertical(th.desktop));
            draw_header(f, cv, &ctx);
            clear(cv, th, &l, l.stage);
            if app.screen == Screen::Page {
                draw_page(f, cv, &ctx, None);
                self.page_drawn = Some((page_start(app), app.reader.shown(), page_highlight(app)));
            } else {
                *word_box = draw_word(f, cv, &ctx);
                draw_context(f, cv, &ctx);
            }
            draw_status(f, cv, &ctx);
            draw_toolbar(f, cv, &ctx);
            damage.push(l.screen);
        } else if let Some(o) = &old {
            if o.header != view.header {
                draw_header(f, cv, &ctx);
                damage.push(l.header);
            }
            if app.screen == Screen::Page {
                let now_drawn = (page_start(app), app.reader.shown(), page_highlight(app));
                if self.page_drawn.as_ref() != Some(&now_drawn) {
                    // Same page, highlight moved: redraw only the lines it left and entered.
                    let partial = self.page_drawn.as_ref().filter(|d| d.0 == now_drawn.0).map(|d| d.1.clone());
                    damage.extend(draw_page(f, cv, &ctx, partial));
                    self.page_drawn = Some(now_drawn);
                }
                if o.status != view.status {
                    draw_status(f, cv, &ctx);
                    damage.push(l.status);
                }
            } else {
                if o.word != view.word {
                    // Clear just the previous word, draw the new one, push both boxes.
                    clear(cv, th, &l, *word_box);
                    let new_box = draw_word(f, cv, &ctx);
                    damage.push(word_box.union(&new_box).intersect(&l.word));
                    *word_box = new_box;
                }
                if o.context != view.context {
                    draw_context(f, cv, &ctx);
                    damage.push(l.context);
                }
                if o.status != view.status {
                    draw_status(f, cv, &ctx);
                    damage.push(l.status);
                }
            }
            if o.toolbar != view.toolbar {
                draw_toolbar(f, cv, &ctx);
                damage.push(l.toolbar);
            }
        }
        self.last = Some(view);
        damage
    }

    /// Splits the book into pages for the current font, size and screen, a
    /// few dozen per frame in the background. Until the count reaches the
    /// reader, the page on screen starts at its paragraph (see `page_start`).
    fn paginate(&mut self, app: &mut App) {
        const PAGES_PER_FRAME: usize = 24;
        let l = self.layout;
        let th = theme(app.settings.theme);
        let area = page_area(th, &l);
        let st = &app.settings;
        let key = (st.font, st.size, app.reader.book.len(), area.w, area.h, st.theme, (st.spacing, st.indent, st.justify, st.phrase));
        if self.pages_key != Some(key) || app.pages.is_empty() {
            app.pages = alloc::vec![0];
            app.pages_done = app.reader.book.is_empty();
            self.pages_key = Some(key);
        }
        let n = app.reader.book.len();
        // A fixed amount of work per frame, however far into the book the
        // reader is: the page on screen never waits for the count.
        for _ in 0..PAGES_PER_FRAME {
            if app.pages_done {
                break;
            }
            let start = *app.pages.last().unwrap() as usize;
            let (_, next) = layout_page(&mut self.fonts, app, area, start, &l);
            if next >= n || next <= start {
                app.pages_done = true;
            } else {
                app.pages.push(next as u32);
            }
        }
    }
}

struct Ctx<'a> {
    th: &'static Theme,
    l: &'a Layout,
    app: &'a App,
    now: u64,
}

fn progress_px(app: &App, l: &Layout) -> i32 {
    (app.reader.permille() as i32 * (l.screen.w - 2 * l.i(12.0))) / 1000
}

/// The chapter you are in, else the book title.
fn header_title(app: &App) -> String {
    let b = &app.reader.book;
    match b.chapter_index(app.reader.pos()) {
        Some(c) => b.chapters()[c].1.clone(),
        None => b.title.clone(),
    }
}

fn up(th: &Theme, s: &str) -> String {
    if th.upper { s.to_uppercase() } else { s.to_string() }
}

/// The paper the text sits on.
fn paper(th: &Theme, l: &Layout) -> Rect {
    match th.chrome {
        Chrome::Scrivener => {
            let m = l.screen.w / 11;
            Rect::new(m, l.stage.y, l.screen.w - 2 * m, l.stage.h)
        }
        // A sunken white document inside the grey window.
        Chrome::Win98 => l.stage.inset(4, 3),
        _ => l.stage,
    }
}

/// Repaints the background under `r` inside the stage (including frames).
fn clear(cv: &mut Canvas, th: &Theme, l: &Layout, r: Rect) {
    let p = paper(th, l);
    match th.chrome {
        Chrome::Scrivener => {
            cv.fill(r, th.desktop[0].1);
            cv.fill(Rect::new(p.right(), p.y, l.i(3.0), p.h).intersect(&r), mix(th.desktop[0].1, BLACK, 90));
            // A visible edge, so the margins read even when light bleeds.
            if th.light_on_dark {
                cv.fill(Rect::new(p.x - 1, p.y, 1, p.h).intersect(&r), th.guide);
                cv.fill(Rect::new(p.right(), p.y, 1, p.h).intersect(&r), th.guide);
            }
        }
        Chrome::Win98 => {
            cv.fill(r.intersect(&l.stage), W98_FACE);
            cv.clip(r);
            bevel(cv, Rect::new(p.x - 2, p.y - 2, p.w + 4, p.h + 4), true);
            cv.unclip();
        }
        _ => {}
    }
    cv.fill(r.intersect(&p), th.page);
}

/// Windows 98 two-pixel bevel around `r` (raised, or sunken when `pressed`).
fn bevel(cv: &mut Canvas, r: Rect, sunken: bool) {
    let (tl_out, tl_in, br_in, br_out) = if sunken { (0x808080, BLACK, 0xDFDFDF, WHITE) } else { (WHITE, 0xDFDFDF, 0x808080, BLACK) };
    cv.fill(Rect::new(r.x, r.y, r.w, 1), tl_out);
    cv.fill(Rect::new(r.x, r.y, 1, r.h), tl_out);
    cv.fill(Rect::new(r.x + 1, r.y + 1, r.w - 2, 1), tl_in);
    cv.fill(Rect::new(r.x + 1, r.y + 1, 1, r.h - 2), tl_in);
    cv.fill(Rect::new(r.x + 1, r.bottom() - 2, r.w - 2, 1), br_in);
    cv.fill(Rect::new(r.right() - 2, r.y + 1, 1, r.h - 2), br_in);
    cv.fill(Rect::new(r.x, r.bottom() - 1, r.w, 1), br_out);
    cv.fill(Rect::new(r.right() - 1, r.y, 1, r.h), br_out);
}

/// Windows' dotted focus rectangle (every other pixel).
fn focus_dots(cv: &mut Canvas, r: Rect) {
    for x in (r.x..r.right()).step_by(2) {
        cv.fill(Rect::new(x, r.y, 1, 1), BLACK);
        cv.fill(Rect::new(x, r.bottom() - 1, 1, 1), BLACK);
    }
    for y in (r.y..r.bottom()).step_by(2) {
        cv.fill(Rect::new(r.x, y, 1, 1), BLACK);
        cv.fill(Rect::new(r.right() - 1, y, 1, 1), BLACK);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Bar,
    Primary,
    Row,
}

/// Draws a button face; returns the colour for its icon/text.
fn button(cv: &mut Canvas, th: &Theme, l: &Layout, r: Rect, kind: Kind, hot: bool) -> Rgb {
    if th.chrome == Chrome::Win98 {
        if kind == Kind::Row && hot {
            // Selected menu row: pushed in, with the classic dotted focus box.
            cv.fill(r, W98_FACE);
            bevel(cv, r, true);
            focus_dots(cv, r.inset(4, 4));
            return BLACK;
        }
        cv.fill(r, W98_FACE);
        if kind == Kind::Primary {
            // The default button gets the extra black frame.
            cv.fill(r, BLACK);
            cv.fill(r.inset(1, 1), W98_FACE);
            bevel(cv, r.inset(1, 1), hot);
        } else {
            bevel(cv, r, hot);
        }
        return BLACK;
    }
    let style: ButtonStyle = match (kind, hot) {
        (Kind::Bar, false) => th.button,
        (Kind::Bar, true) => th.button_hot,
        (Kind::Primary, false) => th.primary,
        (Kind::Primary, true) => th.primary_hot,
        (Kind::Row, false) => th.row,
        (Kind::Row, true) => th.row_sel,
    };
    let rad = l.px(th.radius.max(2.0)) + if kind == Kind::Row { 0.0 } else { l.px(1.0) };
    if th.chrome == Chrome::Mac && !hot {
        cv.rrect_alpha(Rect::new(r.x, r.y + 1, r.w, r.h), rad, Paint::Solid(BLACK), 22);
    }
    cv.rrect(r, rad, Paint::Solid(style.edge));
    let inner = r.inset(1, 1);
    cv.rrect(inner, (rad - 1.0).max(0.0), Paint::Vertical(style.face));
    if th.chrome == Chrome::Vista {
        // Aero gloss: a soft white line just inside the top edge.
        cv.rrect_alpha(Rect::new(inner.x + 1, inner.y, inner.w - 2, 1), 0.0, Paint::Solid(WHITE), 110);
    }
    style.text
}

fn draw_icon_button(cv: &mut Canvas, th: &Theme, l: &Layout, b: &Button, hot: bool) {
    let kind = if b.primary { Kind::Primary } else { Kind::Bar };
    let c = button(cv, th, l, b.rect, kind, hot);
    let off = if th.chrome == Chrome::Win98 && hot { 1.0 } else { 0.0 };
    let size = if b.primary { l.px(20.0) } else { l.px(17.0) };
    icons::draw(cv, b.icon, b.rect.cx() + off, b.rect.cy() + off, size, c);
}

fn bar_bg(cv: &mut Canvas, th: &Theme, r: Rect, top_edge: bool) {
    match th.chrome {
        Chrome::Vista => {
            cv.paint_rect(r, Paint::Vertical(th.bar));
            cv.fill(Rect::new(r.x, r.y, r.w, 1), mix(th.bar[0].1, WHITE, 90));
            let seam = if top_edge { r.y } else { r.bottom() - 1 };
            cv.fill(Rect::new(r.x, seam, r.w, 1), 0x071521);
        }
        Chrome::Mac => {
            cv.paint_rect(r, Paint::Vertical(th.bar));
            let seam = if top_edge { r.y } else { r.bottom() - 1 };
            cv.fill(Rect::new(r.x, seam, r.w, 1), 0xD1D1D1);
        }
        Chrome::Scrivener => cv.paint_rect(r, Paint::Vertical(th.bar)),
        Chrome::Win98 => {
            if top_edge {
                // The grey window body with a raised top edge.
                cv.fill(r, W98_FACE);
                cv.fill(Rect::new(r.x, r.y, r.w, 1), WHITE);
            } else {
                cv.fill(r, W98_FACE);
                cv.paint_rect(r.inset(2, 2), Paint::Horizontal(th.bar));
            }
        }
        Chrome::Ink => {
            cv.fill(r, th.page);
            let seam = if top_edge { r.y } else { r.bottom() - 2 };
            cv.fill(Rect::new(r.x + 8, seam, r.w - 16, 1), th.text);
        }
    }
}

/// Text colours on the bottom bar.
fn bar_label_colours(th: &Theme) -> (Rgb, Rgb) {
    match th.chrome {
        Chrome::Win98 => (BLACK, 0x404040),
        _ => (th.bar_text, th.bar_dim),
    }
}

// ------------------------------------------------------------------ header

fn draw_header(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    let r = l.header;
    match th.chrome {
        Chrome::Vista => {
            // Aero: the aurora shows through the glass title area.
            cv.paint_rect(r, Paint::Vertical(&[(0, 0x6F9CC4), (110, 0x3E6E99), (255, 0x234A72)]));
            cv.clip(r);
            cv.glow(r.w as f32 * 0.85, r.y as f32 - 6.0, r.w as f32 * 0.35, l.px(26.0), 0x2A7FC0, 140);
            cv.glow(r.w as f32 * 0.25, r.bottom() as f32 + 4.0, r.w as f32 * 0.3, l.px(22.0), 0x3AA86B, 90);
            cv.unclip();
            cv.fill(Rect::new(0, r.y, r.w, 1), 0xA9C6E2);
            cv.fill(Rect::new(0, r.bottom() - 1, r.w, 1), 0x0B1B28);
        }
        _ => bar_bg(cv, th, r, false),
    }
    let title_c = match th.chrome {
        Chrome::Vista | Chrome::Win98 => WHITE,
        _ => th.bar_text,
    };
    // Menu button, top-left.
    let mb = menu_button(l);
    draw_icon_button(cv, th, l, &mb, app.lit(c.now) == Some(mb.hit));
    let mut x = mb.rect.right() as f32 + l.px(8.0);
    if th.chrome == Chrome::Mac {
        for (i, col) in [0xFF5F57, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
            let cx = x + l.px(5.0) + i as f32 * l.px(15.0);
            cv.circle(cx, r.cy(), l.px(5.0), Paint::Solid(mix(col, BLACK, 40)));
            cv.circle(cx, r.cy(), l.px(4.3), Paint::Solid(col));
        }
        x += l.px(48.0);
    }
    let base = r.cy() + l.px(4.5);
    // Battery, top-right.
    let mut right = r.w as f32 - l.px(8.0);
    let small = l.px(11.0);
    match app.battery {
        Some(b) => {
            let bw = l.px(20.0);
            let bh = l.px(10.0);
            let bx = right - bw;
            let br = Rect::new(bx as i32, (r.cy() - bh / 2.0) as i32, (bw - l.px(2.0)) as i32, bh as i32);
            cv.rrect_stroke(br, l.px(2.0).min(th.radius.max(0.5)), title_c);
            cv.fill(Rect::new(br.right(), (r.cy() - l.px(2.0)) as i32, l.i(2.0), l.i(4.0)), title_c);
            let inner = ((br.w - 4) as f32 * b.percent.min(100) as f32 / 100.0) as i32;
            let low = b.percent <= 15 && !b.charging;
            cv.fill(Rect::new(br.x + 2, br.y + 2, inner, br.h - 4), if low { 0xE0453A } else { title_c });
            let txt = format!("{}%", b.percent);
            right = bx - l.px(5.0) - f.measure(th.ui.0, small, &txt);
            f.draw(cv, th.ui.0, small, &txt, right, base, title_c);
        }
        None => {
            let txt = up(th, "USB");
            right -= f.measure(th.ui.0, small, &txt);
            f.draw(cv, th.ui.0, small, &txt, right, base, title_c);
        }
    }
    right -= l.px(8.0);
    if app.screen == Screen::Page {
        // The highlight dot: a ring when off, filled when on.
        let hb = highlight_button(l);
        let (cx, cy, rad) = (hb.rect.cx(), hb.rect.cy(), hb.rect.w as f32 / 2.0);
        if app.lit(c.now) == Some(Hit::Highlight) {
            cv.circle(cx, cy, rad + l.px(3.0), Paint::Solid(mix(th.bar[0].1, title_c, 60)));
        }
        cv.ring(cx, cy, rad - 1.0, l.px(1.6), title_c);
        if app.settings.highlight {
            cv.circle(cx, cy, rad - l.px(4.5), Paint::Solid(title_c));
        }
        right = right.min(hb.hit_rect.x as f32 - l.px(2.0));
    }
    // Messages take the title's place for a moment, then the title returns.
    if app.toast().is_some() {
        draw_toast(f, cv, c, Rect::new(x as i32, r.y, (right - x) as i32, r.h));
        return;
    }
    let title = up(th, &header_title(app));
    let px = l.px(12.5);
    f.draw_fit(cv, th.ui.1, px, &title, x, base, right - x, title_c);
}

// ------------------------------------------------------------------ word

fn word_px(c: &Ctx) -> f32 {
    c.l.px(WORD_PX[c.app.settings.size.min(2) as usize])
}

/// Baseline that centres the word vertically in its area.
fn word_baseline(f: &mut Fonts, c: &Ctx) -> f32 {
    let m = f.line(family(c.app.settings.font).face(Style::PLAIN), word_px(c));
    c.l.word.cy() + (m.ascent - m.descent) / 2.0
}

/// Draws the word (or phrase), centred; returns the box it covers.
fn draw_word(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) -> Rect {
    let (th, l, app) = (c.th, c.l, c.app);
    let book = &app.reader.book;
    let p = paper(th, l);
    if book.is_empty() {
        let s = up(th, "No book loaded");
        let w = f.measure(th.ui.0, l.px(14.0), &s);
        f.draw(cv, th.ui.0, l.px(14.0), &s, p.cx() - w / 2.0, l.word.cy(), th.dim);
        return l.word.intersect(&p);
    }
    if let Screen::Line { .. } = app.screen {
        clear(cv, th, l, l.word);
        draw_line(f, cv, c);
        return l.word.intersect(&p);
    }
    let mode = app.settings.mode();
    if mode == Mode::Ticker {
        // Only the strip the line of text occupies is redrawn each frame.
        let band = ticker_band(f, c).intersect(&l.word.intersect(&p));
        clear(cv, th, l, band);
        draw_ticker(f, cv, c);
        return band;
    }
    let fam = family(app.settings.font);
    let words: Vec<(&str, Face, bool)> = app
        .reader
        .shown()
        .map(|i| {
            let w = book.word(i).unwrap();
            (book.text_of(w), fam.face(w.style), w.style.heading())
        })
        .collect();
    let heading = words.iter().any(|w| w.2);
    // Phrases are set a little smaller so a whole group fits on one line.
    let phrase = mode == Mode::Phrase;
    let mut px = word_px(c) * if phrase { PHRASE_SCALE } else { 1.0 };
    let left = p.x as f32 + phrase_left(l, p);
    let avail = if phrase { p.right() as f32 - l.px(14.0) - left } else { (p.w as f32 - l.px(28.0)).max(10.0) };
    let measure = |f: &mut Fonts, px: f32| -> f32 {
        let space = f.measure(words[0].1, px, " ");
        words.iter().map(|(s, face, _)| f.measure(*face, px, s)).sum::<f32>() + space * (words.len() - 1) as f32
    };
    let mut total = measure(f, px);
    if total > avail {
        // Long words shrink to fit instead of being cut off.
        px *= (avail / total).max(0.5);
        total = measure(f, px);
    }
    let base = word_baseline(f, c);
    let m = f.line(fam.face(Style::PLAIN), px);
    let colour = if heading { th.heading } else { th.text };
    // Word mode centres; Phrase mode starts every group at the same left
    // point, so the eye returns to one spot and only the right edge moves
    // (as on the original e-reader).
    let x0 = if phrase { left } else { p.cx() - total.min(avail) / 2.0 };
    let mut x = x0;
    cv.clip(l.word.intersect(&p));
    let space = f.measure(words[0].1, px, " ");
    for (s, face, _) in &words {
        x += f.draw(cv, *face, px, s, x, base, colour) + space;
    }
    // Chapter titles: bold (from the markup) and the heading colour. No ornaments.
    cv.unclip();
    let top = base - m.ascent - l.px(3.0);
    let pad = l.px(3.0);
    Rect::new((x0 - pad) as i32, top as i32, (total.min(avail) + 2.0 * pad) as i32 + 1, (m.ascent + m.descent + l.px(6.0)) as i32 + 1)
        .intersect(&l.word.intersect(&p))
}

/// Phrase and ticker text size relative to the single-word size.
const PHRASE_SCALE: f32 = 0.72;

/// Where Phrase-mode groups start: a tenth of the way in, like the
/// original's fixed left edge.
fn phrase_left(l: &Layout, p: Rect) -> f32 {
    (p.w as f32 * 0.10).max(l.px(18.0))
}

/// The horizontal strip the ticker's text covers.
fn ticker_band(f: &mut Fonts, c: &Ctx) -> Rect {
    let px = word_px(c) * PHRASE_SCALE;
    let m = f.line(family(c.app.settings.font).face(Style::PLAIN), px);
    let base = c.l.word.cy() + (m.ascent - m.descent) / 2.0;
    let pad = c.l.px(3.0);
    Rect::new(c.l.word.x, (base - m.ascent - pad) as i32, c.l.word.w, (m.ascent + m.descent + 2.0 * pad) as i32 + 1)
}

/// Ticker: the text glides right to left as one line at reading speed. The
/// word being read sits a third of the way in; between words it slides
/// smoothly (reader progress), so the eye can stay still.
fn draw_ticker(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    let book = &app.reader.book;
    let fam = family(app.settings.font);
    let px = word_px(c) * PHRASE_SCALE;
    let p = paper(th, l);
    let pos = app.reader.pos();
    let base = l.word.cy() + {
        let m = f.line(fam.face(Style::PLAIN), px);
        (m.ascent - m.descent) / 2.0
    };
    let space = f.measure(fam.face(Style::PLAIN), px, " ");
    let word = |i: usize| book.word(i).unwrap();
    let cur = word(pos);
    let cw = f.measure(fam.face(cur.style), px, book.text_of(cur));
    let anchor = p.x as f32 + p.w as f32 * 0.32;
    let x_cur = anchor - app.reader.progress(c.now) * (cw + space);
    cv.clip(l.word.intersect(&p));
    let colour = |w: reader_core::Word, past: bool| {
        if past { th.dim } else if w.style.heading() { th.heading } else { th.text }
    };
    let mut x = x_cur;
    let mut i = pos;
    while x < p.right() as f32 && i < book.len() {
        let w = word(i);
        x += f.draw(cv, fam.face(w.style), px, book.text_of(w), x, base, colour(w, false)) + space;
        i += 1;
    }
    let mut x = x_cur - space;
    let mut i = pos;
    while x > p.x as f32 && i > 0 {
        i -= 1;
        let w = word(i);
        let s = book.text_of(w);
        let ww = f.measure(fam.face(w.style), px, s);
        f.draw(cv, fam.face(w.style), px, s, x - ww, base, colour(w, true));
        x -= ww + space;
    }
    cv.unclip();
}

/// Hold-to-scroll: the text as one line, current word highlighted at the centre.
fn draw_line(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    let book = &app.reader.book;
    let fam = family(app.settings.font);
    let px = word_px(c) * 0.62;
    let p = paper(th, l);
    let pos = app.reader.pos();
    let base = word_baseline(f, c);
    let m = f.line(fam.face(Style::PLAIN), px);
    let space = f.measure(fam.face(Style::PLAIN), px, " ");
    let word = |i: usize| book.word(i).unwrap();
    let cur = word(pos);
    let cur_s = book.text_of(cur);
    let cw = f.measure(fam.face(cur.style), px, cur_s);
    let x0 = p.cx() - cw / 2.0;
    cv.clip(l.word.intersect(&p));
    let pad = l.px(4.0);
    let hl = Rect::new((x0 - pad) as i32, (base - m.ascent - pad / 2.0) as i32, (cw + 2.0 * pad) as i32, (m.ascent + m.descent + pad) as i32);
    cv.rrect(hl, th.radius.max(2.0), Paint::Solid(th.mark));
    f.draw(cv, fam.face(cur.style), px, cur_s, x0, base, th.mark_text);
    let mut x = x0 + cw + space;
    let mut i = pos + 1;
    while x < p.right() as f32 && i < book.len() {
        let w = word(i);
        x += f.draw(cv, fam.face(w.style), px, book.text_of(w), x, base, th.text) + space;
        i += 1;
    }
    let mut x = x0 - space;
    let mut i = pos;
    while x > p.x as f32 && i > 0 {
        i -= 1;
        let w = word(i);
        let s = book.text_of(w);
        let ww = f.measure(fam.face(w.style), px, s);
        f.draw(cv, fam.face(w.style), px, s, x - ww, base, th.dim);
        x -= ww + space;
    }
    cv.unclip();
}

/// Under the word, only while paused: the sentence around it (RSVP's known
/// weakness is that you can't glance back; this restores the glance).
fn draw_context(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    clear(cv, th, l, l.context);
    let p = paper(th, l);
    let r = l.context.intersect(&p);
    if let Screen::Line { .. } = app.screen {
        let s = up(th, "Release to keep reading from here");
        let w = f.measure(th.ui.0, l.px(11.0), &s);
        f.draw(cv, th.ui.0, l.px(11.0), &s, r.cx() - w / 2.0, r.cy() + l.px(4.0), th.dim);
        return;
    }
    if app.screen != Screen::Reading || app.reader.is_playing() || app.reader.book.is_empty() || !app.settings.context {
        return;
    }
    let book = &app.reader.book;
    let fam = family(app.settings.font);
    let px = l.px(12.5);
    let face = fam.face(Style::PLAIN);
    let space = f.measure(face, px, " ");
    let shown = app.reader.shown();
    let start = book.sentence_start(shown.start).max(shown.start.saturating_sub(7));
    let max_w = r.w as f32 - l.px(28.0);
    let mut lines: Vec<Vec<(usize, f32)>> = alloc::vec![Vec::new()];
    let mut lw = 0.0;
    let mut i = start;
    while i < book.len() {
        let w = book.word(i).unwrap();
        let ww = f.measure(fam.face(w.style), px, book.text_of(w));
        if lw + ww > max_w && !lines.last().unwrap().is_empty() {
            if lines.len() == 2 {
                break;
            }
            lines.push(Vec::new());
            lw = 0.0;
        }
        lines.last_mut().unwrap().push((i, ww));
        lw += ww + space;
        if w.pause >= Pause::Sentence && i + 1 >= shown.end {
            break;
        }
        i += 1;
    }
    let lh = l.px(15.0);
    let top = r.y as f32 + (r.h as f32 - lh * lines.len() as f32) / 2.0 + lh * 0.78;
    for (n, line) in lines.iter().enumerate() {
        let width: f32 = line.iter().map(|w| w.1).sum::<f32>() + space * (line.len().saturating_sub(1)) as f32;
        let x0 = r.cx() - width / 2.0;
        let y = top + n as f32 * lh;
        // One highlight shape around the whole group on this line.
        let mut x = x0;
        let mut span: Option<(f32, f32)> = None;
        for &(idx, ww) in line {
            if shown.contains(&idx) {
                span = Some(span.map_or((x, x + ww), |(a, _)| (a, x + ww)));
            }
            x += ww + space;
        }
        if let Some((a, z)) = span {
            let hr = Rect::new((a - 2.0) as i32, (y - lh * 0.78) as i32, (z - a + 4.0) as i32, (lh * 0.98) as i32);
            cv.rrect(hr, th.radius.min(3.0), Paint::Solid(th.mark));
        }
        let mut x = x0;
        for &(idx, ww) in line {
            let w = book.word(idx).unwrap();
            let here = shown.contains(&idx);
            f.draw(cv, fam.face(w.style), px, book.text_of(w), x, y, if here { th.mark_text } else { th.dim });
            x += ww + space;
        }
    }
}

// ------------------------------------------------------------------ status

/// A thin progress line always; the numbers only while paused.
fn draw_status(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    clear(cv, th, l, l.status);
    let p = paper(th, l);
    let pad = l.i(12.0);
    let bar = Rect::new(p.x + pad, l.status.bottom() - l.i(8.0), p.w - 2 * pad, l.i(3.0).max(2));
    let rad = (bar.h as f32 / 2.0).min(th.radius.max(0.5));
    cv.rrect(bar, rad, Paint::Solid(th.track));
    let filled = (app.reader.permille() as i32 * bar.w) / 1000;
    if filled > 0 {
        cv.rrect(Rect::new(bar.x, bar.y, filled.max(bar.h), bar.h), rad, Paint::Solid(th.fill));
    }
    if app.reader.is_playing() {
        return;
    }
    let base = bar.y as f32 - l.px(4.0);
    let px = l.px(10.5);
    let b = &app.reader.book;
    let left = match (app.screen, app.page_index()) {
        (Screen::Page, Some(i)) if app.pages_done => format!("Page {} of {}  ·  {}%", i + 1, app.pages.len(), app.reader.permille() / 10),
        (Screen::Page, Some(i)) => format!("Page {}  ·  {}%", i + 1, app.reader.permille() / 10),
        (Screen::Page, None) => format!("Page …  ·  {}%  ·  counting pages", app.reader.permille() / 10),
        _ => {
            let mins = app.reader.ms_left() / 60_000;
            let left = if mins == 0 { String::from("< 1 min left") } else { format!("{mins} min left") };
            format!("{}%  ·  word {} of {}  ·  {left}", app.reader.permille() / 10, group(app.reader.pos() + 1), group(b.len()))
        }
    };
    let left = up(th, &left);
    let lw = f.draw_fit(cv, th.ui.0, px, &left, bar.x as f32, base, bar.w as f32, th.dim);
    if let Some(ci) = b.chapter_index(app.reader.pos()) {
        let s = up(th, &format!("ch. {} / {}", ci + 1, b.chapters().len()));
        let w = f.measure(th.ui.0, px, &s);
        if w < bar.w as f32 - lw - l.px(12.0) {
            f.draw(cv, th.ui.0, px, &s, bar.right() as f32 - w, base, th.dim);
        }
    }
}

/// 12345 → "12,345".
fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

// ------------------------------------------------------------------ toolbar

fn draw_toolbar(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    bar_bg(cv, th, l.toolbar, true);
    let (buttons, label) = toolbar(app, l);
    let lit = app.lit(c.now);
    for b in &buttons {
        draw_icon_button(cv, th, l, b, lit == Some(b.hit));
    }
    if let Some(lb) = label {
        let (fg, dim) = bar_label_colours(th);
        let big = l.px(15.0);
        let w = f.measure(th.ui.1, big, &lb.big);
        f.draw(cv, th.ui.1, big, &lb.big, lb.rect.cx() - w / 2.0, lb.rect.cy() + l.px(2.0), fg);
        let small = up(th, &lb.small);
        let sw = f.measure(th.ui.0, l.px(9.5), &small);
        f.draw(cv, th.ui.0, l.px(9.5), &small, lb.rect.cx() - sw / 2.0, lb.rect.cy() + l.px(13.0), dim);
    }
}

// ------------------------------------------------------------------ toast

pub fn toast_text(app: &App, fb: Feedback) -> String {
    let limit = fb.tone == Tone::Limit;
    match fb.msg {
        Msg::Playing => "Reading".into(),
        Msg::Paused => "Paused".into(),
        Msg::Speed(n) if limit && n <= reader_core::reader::MIN_WPM => format!("Slowest: {n} wpm"),
        Msg::Speed(n) if limit => format!("Fastest: {n} wpm"),
        Msg::Speed(n) => format!("{n} words per minute"),
        Msg::Skipped { forward, skip } => {
            let what = match skip {
                Skip::Sentence => "1 sentence".to_string(),
                Skip::Words(n) => format!("{n} words"),
                Skip::Paragraph => "1 paragraph".to_string(),
            };
            format!("{} {what}", if forward { "Forward" } else { "Back" })
        }
        Msg::PageBack => "Previous page".into(),
        Msg::PageForward => "Next page".into(),
        Msg::AtStart => "Start of book".into(),
        Msg::AtEnd => "End of book".into(),
        Msg::Top => "Top of list".into(),
        Msg::Bottom => "End of list".into(),
        Msg::Set(kind, i) => format!("{}: {}", kind.title(), app.pick_label(kind, i)),
        Msg::Kept => "Kept as before".into(),
        Msg::Restarted => "From the start".into(),
        Msg::Finished => "Finished. Play starts over".into(),
        Msg::Sleeping => "Going to sleep".into(),
        Msg::PagesView => "Page view".into(),
        Msg::SpeedView => "Speed view".into(),
        Msg::Rotated => "Screen rotated".into(),
        Msg::NoChapters => "This book has no chapters".into(),
        Msg::Highlight(true) => "Highlight on".into(),
        Msg::Highlight(false) => "Highlight off".into(),
        Msg::Wifi(true) => "Web view on: join Wi-Fi FastReader".into(),
        Msg::Wifi(false) => "Web view off".into(),
        Msg::Shown => String::new(),
    }
}

/// A message chip centred in `area` (the title bar).
fn draw_toast(f: &mut Fonts, cv: &mut Canvas, c: &Ctx, area: Rect) {
    let (th, l, app) = (c.th, c.l, c.app);
    let Some(t) = app.toast() else { return };
    let text = up(th, &toast_text(app, t.feedback));
    let px = l.px(12.0);
    let pad = l.px(12.0);
    let tw = f.measure(th.ui.1, px, &text).min(area.w as f32 - 2.0 * pad);
    let w = tw + 2.0 * pad;
    let h = (area.h as f32 - l.px(5.0)).min(l.px(22.0));
    let r = Rect::new((area.cx() - w / 2.0) as i32, (area.cy() - h / 2.0) as i32, w as i32, h as i32);
    let limit = t.feedback.tone == Tone::Limit;
    let (bg, fg) = if limit { (th.limit, th.limit_text) } else { (th.toast, th.toast_text) };
    let radius = match th.chrome {
        Chrome::Mac => h / 2.0,
        Chrome::Win98 => 0.0,
        Chrome::Ink => 2.0,
        _ => l.px(4.0),
    };
    let edge = if limit && th.chrome != Chrome::Win98 { mix(th.limit, BLACK, 50) } else { th.toast_edge };
    cv.rrect(r, radius, Paint::Solid(edge));
    let inner = r.inset(1, 1);
    if th.chrome == Chrome::Vista && !limit {
        cv.rrect(inner, (radius - 1.0).max(0.0), Paint::Vertical(&[(0, 0xF6F9FD), (255, 0xE3EBF5)]));
    } else {
        cv.rrect(inner, (radius - 1.0).max(0.0), Paint::Solid(bg));
    }
    f.draw_fit(cv, th.ui.1, px, &text, r.cx() - tw / 2.0, r.cy() + px * 0.36, tw + 1.0, fg);
}

// ------------------------------------------------------------------ page view

fn page_area(th: &Theme, l: &Layout) -> Rect {
    let p = paper(th, l);
    Rect::new(p.x + l.i(14.0), p.y + l.i(8.0), p.w - l.i(28.0), l.status.y - p.y - l.i(10.0))
}

struct Placed {
    idx: usize,
    x: f32,
    base: f32,
    w: f32,
    face: Face,
    px: f32,
}

/// Lays out one page starting at word `start`; returns the words and where
/// the next page starts. Pagination and drawing share this, so they agree.
///
/// Typography follows the reader's settings: line spacing (tight / normal /
/// airy), paragraphs spaced apart or indented like a printed book (no indent
/// after a heading, as in print), and ragged or justified lines (a line is
/// never stretched by more than a sixth, and paragraph ends stay ragged).
fn layout_page(f: &mut Fonts, app: &App, area: Rect, start: usize, l: &Layout) -> (Vec<Placed>, usize) {
    let book = &app.reader.book;
    let st = &app.settings;
    let fam = family(st.font);
    let base_px = l.px(PAGE_PX[st.size.min(2) as usize]);
    let leading = [1.18, 1.36, 1.6][st.spacing.min(2) as usize];
    let indent = base_px * 1.6;
    let mut out: Vec<Placed> = Vec::new();
    let mut y = area.y as f32;
    let mut line: Vec<Placed> = Vec::new();
    let mut x = 0.0;
    let mut line_heading = false;
    let mut i = start;
    let mut gap_before = 0.0;

    // Finishes a line; returns false if it doesn't fit on the page.
    let flush = |line: &mut Vec<Placed>, out: &mut Vec<Placed>, y: &mut f32, heading: bool, gap: f32, last: bool, f: &mut Fonts| -> bool {
        if line.is_empty() {
            return true;
        }
        let m = f.line(line[0].face, line[0].px);
        let lh = m.height * if heading { 1.15 } else { leading };
        if *y + gap + lh > area.bottom() as f32 && !out.is_empty() {
            return false;
        }
        *y += gap;
        let base = *y + m.ascent;
        // Justify: share the leftover width between the gaps.
        let n = line.len();
        let right = line[n - 1].x + line[n - 1].w;
        let spare = area.right() as f32 - right;
        let stretch = if st.justify && !last && !heading && n > 1 && spare < area.w as f32 / 6.0 { spare / (n - 1) as f32 } else { 0.0 };
        for (k, mut p) in line.drain(..).enumerate() {
            p.base = base;
            p.x += stretch * k as f32;
            out.push(p);
        }
        *y += lh;
        true
    };
    let next_of = |out: &Vec<Placed>| out.last().map_or(start + 1, |p| p.idx + 1);

    while i < book.len() {
        let w = book.word(i).unwrap();
        let heading = w.style.heading();
        let px = if heading { base_px * 1.3 } else { base_px };
        let face = fam.face(w.style);
        let ww = f.measure(face, px, book.text_of(w));
        let space = f.measure(face, px, " ");
        if !line.is_empty() && (heading != line_heading || x + ww > area.w as f32) {
            if !flush(&mut line, &mut out, &mut y, line_heading, gap_before, false, f) {
                let next = next_of(&out);
                return (out, next);
            }
            gap_before = 0.0;
            x = 0.0;
        }
        if line.is_empty() {
            line_heading = heading;
            // First line of a paragraph, book style: indent (not after a heading).
            let para_start = i == 0 || book.word(i - 1).is_some_and(|p| p.pause == Pause::Paragraph);
            let after_heading = i > 0 && book.word(i - 1).is_some_and(|p| p.style.heading());
            if st.indent && para_start && !heading && !after_heading && i > 0 {
                x = indent;
            }
        }
        line.push(Placed { idx: i, x: area.x as f32 + x, base: 0.0, w: ww, face, px });
        x += ww + space;
        i += 1;
        if w.pause == Pause::Paragraph {
            if !flush(&mut line, &mut out, &mut y, line_heading, gap_before, true, f) {
                let next = next_of(&out);
                return (out, next);
            }
            gap_before = if st.indent && !heading { 0.0 } else { base_px * 0.6 };
            x = 0.0;
        }
    }
    if !flush(&mut line, &mut out, &mut y, line_heading, gap_before, true, f) {
        let next = next_of(&out);
        return (out, next);
    }
    (out, book.len())
}

/// First word of the page on screen.
fn page_start(app: &App) -> usize {
    match app.page_index() {
        Some(i) => app.pages[i] as usize,
        None => app.reader.book.paragraph_start(app.reader.pos()),
    }
}

/// Whether the current phrase is highlighted on the page.
fn page_highlight(app: &App) -> bool {
    app.reader.is_playing() || app.settings.highlight
}

/// Draws the page. With `moved_from` (the previously highlighted phrase),
/// only the lines that phrase or the current one touch are redrawn.
/// Returns the damaged rectangles.
fn draw_page(f: &mut Fonts, cv: &mut Canvas, c: &Ctx, moved_from: Option<core::ops::Range<usize>>) -> Vec<Rect> {
    let (th, l, app) = (c.th, c.l, c.app);
    let area = page_area(th, l);
    let text_area = Rect::new(l.stage.x, l.stage.y, l.stage.w, l.status.y - l.stage.y);
    let start = page_start(app);
    let (placed, _) = layout_page(f, app, area, start, l);
    let book = &app.reader.book;
    let pos = app.reader.pos();
    let phrase = app.reader.shown();
    let show = page_highlight(app);

    // Group the placed words into lines (same baseline).
    let mut lines: Vec<(usize, usize)> = Vec::new();
    for (k, p) in placed.iter().enumerate() {
        match lines.last_mut() {
            Some(line) if placed[line.0].base == p.base => line.1 = k,
            _ => lines.push((k, k)),
        }
    }
    let line_rect = |f: &mut Fonts, a: usize| {
        let p = &placed[a];
        let m = f.line(p.face, p.px);
        Rect::new(text_area.x, (p.base - m.ascent - l.px(2.0)) as i32, text_area.w, (m.ascent + m.descent + l.px(4.0)) as i32)
    };
    let mut damage = Vec::new();
    match &moved_from {
        None => {
            clear(cv, th, l, text_area);
            damage.push(text_area);
        }
        Some(old) => {
            for &(a, z) in &lines {
                let touches = (a..=z).any(|k| phrase.contains(&placed[k].idx) || old.contains(&placed[k].idx));
                if touches {
                    let r = line_rect(f, a).intersect(&text_area);
                    clear(cv, th, l, r);
                    damage.push(r);
                }
            }
        }
    }
    for &(a, z) in &lines {
        let r = line_rect(f, a);
        if moved_from.is_some() && !damage.iter().any(|d| *d == r.intersect(&text_area)) {
            continue;
        }
        // The phrase being read: one highlight shape per line.
        if show {
            let lit: Vec<usize> = (a..=z).filter(|&k| phrase.contains(&placed[k].idx)).collect();
            if let (Some(&first), Some(&last)) = (lit.first(), lit.last()) {
                let (p0, p1) = (&placed[first], &placed[last]);
                let m = f.line(p0.face, p0.px);
                let pad = l.px(2.5);
                let hr = Rect::new((p0.x - pad) as i32, (p0.base - m.ascent - pad * 0.4) as i32, (p1.x + p1.w - p0.x + 2.0 * pad) as i32, (m.ascent + m.descent + pad) as i32);
                cv.rrect(hr, th.radius.clamp(1.0, 4.0), Paint::Solid(th.mark));
            }
        }
        for p in &placed[a..=z] {
            let w = book.word(p.idx).unwrap();
            let lit = show && phrase.contains(&p.idx);
            let colour = if lit { th.mark_text } else if w.style.heading() { th.heading } else { th.text };
            if !show && p.idx == pos && pos != start {
                // Where speed reading left off.
                cv.fill(Rect::new(p.x as i32, (p.base + l.px(3.0)) as i32, p.w as i32, l.i(2.0).max(1)), th.fill);
            }
            f.draw(cv, p.face, p.px, book.text_of(w), p.x, p.base, colour);
        }
    }
    damage
}

// ------------------------------------------------------------------ lists

fn draw_list(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) {
    let (th, l, app) = (c.th, c.l, c.app);
    let g = list_geom(app, l);
    let lit = app.lit(c.now);
    cv.paint_rect(Rect::new(0, g.header.bottom(), l.screen.w, l.screen.h - g.header.bottom()), Paint::Vertical(th.list));
    match th.chrome {
        Chrome::Vista => {
            cv.paint_rect(g.header, Paint::Vertical(&[(0, 0x6F9CC4), (110, 0x3E6E99), (255, 0x234A72)]));
            cv.fill(Rect::new(0, g.header.bottom() - 1, g.header.w, 1), 0x0B1B28);
        }
        _ => bar_bg(cv, th, g.header, false),
    }
    let title_c = match th.chrome {
        Chrome::Vista | Chrome::Win98 => WHITE,
        _ => th.bar_text,
    };
    for b in [&g.back, &g.close] {
        draw_icon_button(cv, th, l, b, lit == Some(b.hit));
    }
    if app.toast().is_some() {
        // In lists the message takes the title's place, so it never covers a row.
        let area = Rect::new(g.back.rect.right(), g.header.y, g.close.rect.x - g.back.rect.right(), g.header.h);
        draw_toast(f, cv, c, area);
    } else {
        let title = up(th, &g.title);
        let tw = f.measure(th.ui.1, l.px(13.0), &title);
        f.draw(cv, th.ui.1, l.px(13.0), &title, g.header.cx() - tw / 2.0, g.header.cy() + l.px(4.5), title_c);
    }

    let sel = match app.screen {
        Screen::Menu { sel } | Screen::Pick { sel, .. } => Some(sel),
        _ => None,
    };
    let original = match app.screen {
        Screen::Pick { original, .. } => Some(original),
        _ => None,
    };
    let fam_list = layout::is_font_list(app);
    let text_c = th.row.text;
    for row in &g.rows {
        let r = row.rect;
        match &row.kind {
            RowKind::Section(name) => {
                let s = up(th, name);
                f.draw(cv, th.ui.1, l.px(10.5), &s, r.x as f32 + l.px(4.0), r.bottom() as f32 - l.px(5.0), th.section);
            }
            RowKind::Info(k, v) => {
                f.draw(cv, th.ui.0, l.px(12.0), &up(th, k), r.x as f32 + l.px(6.0), r.cy() + l.px(4.0), mix(text_c, th.list[0].1, 110));
                let max = r.w as f32 * 0.64;
                let vw = f.measure(th.ui.0, l.px(12.0), v).min(max);
                f.draw_fit(cv, th.ui.0, l.px(12.0), v, r.right() as f32 - l.px(6.0) - vw, r.cy() + l.px(4.0), max, text_c);
                cv.fill(Rect::new(r.x, r.bottom() - 1, r.w, 1), mix(text_c, th.list[0].1, 215));
            }
            RowKind::Item(i) => {
                let hot = sel == Some(*i) || lit == Some(Hit::Row(*i));
                let fg = button(cv, th, l, r, Kind::Row, hot);
                let dim = if hot { fg } else { mix(fg, th.row.face[0].1, 100) };
                let (label, value, sub) = layout::row_text(app, *i);
                let mut x = r.x as f32 + l.px(10.0);
                if let Some(o) = original {
                    // Radio: the dot marks what was set before you started browsing.
                    let rcx = x + l.px(4.0);
                    cv.ring(rcx, r.cy(), l.px(5.0), 1.2, dim);
                    if o == *i {
                        cv.circle(rcx, r.cy(), l.px(2.6), Paint::Solid(fg));
                    }
                    x += l.px(16.0);
                }
                let label = up(th, &label);
                let base = r.cy() + l.px(4.8);
                if fam_list {
                    // Each font name is shown in its own font.
                    let face = family(*i).face(Style::PLAIN);
                    f.draw_fit(cv, face, l.px(15.0), &label, x, base, r.w as f32 * 0.88, fg);
                } else {
                    f.draw_fit(cv, th.ui.0, l.px(13.5), &label, x, base, r.w as f32 * 0.58, fg);
                }
                let mut right = r.right() as f32 - l.px(9.0);
                if sub {
                    icons::draw(cv, Icon::PageForward, right - l.px(4.0), r.cy(), l.px(11.0), dim);
                    right -= l.px(14.0);
                }
                if let Some(v) = value {
                    let v = up(th, &v);
                    let max = r.w as f32 * 0.38;
                    let vw = f.measure(th.ui.0, l.px(12.0), &v).min(max);
                    f.draw_fit(cv, th.ui.0, l.px(12.0), &v, right - vw, base, max, dim);
                }
            }
        }
    }
    if let Some((upb, downb)) = &g.scroll {
        for b in [upb, downb] {
            let hot = lit == Some(b.hit);
            let fg = button(cv, th, l, b.rect, Kind::Row, hot);
            icons::draw(cv, b.icon, b.rect.cx(), b.rect.cy(), l.px(16.0), fg);
        }
        if let Some((a, z)) = g.thumb {
            let col = upb.hit_rect;
            let x = col.x + col.w / 2 - l.i(1.5);
            let y0 = upb.rect.bottom() + l.i(3.0);
            let h = (downb.rect.y - l.i(3.0) - y0) as f32;
            cv.rrect(Rect::new(x, y0, l.i(3.0).max(2), h as i32), 1.5, Paint::Solid(mix(text_c, th.list[0].1, 200)));
            cv.rrect(Rect::new(x, y0 + (a * h) as i32, l.i(3.0).max(2), ((z - a) * h).max(6.0) as i32), 1.5, Paint::Solid(mix(text_c, th.list[0].1, 90)));
        }
    }
}

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
use crate::layout::{self, bookmark_corner, paper, highlight_button, hit_test, list_geom, header_buttons, toolbar, Button, Icon, Layout, RowKind};
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
    word: (usize, u8, bool, u16, bool),
    context: Option<(usize, i16)>,
    status: (i32, u64, Option<usize>, Option<usize>, bool),
    toolbar: (bool, Option<Hit>, u16, PageInfo),
    /// The current place (word, or page in page view) has a bookmark.
    corner: bool,
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
    /// The sentence under the word, when longer than fits: whether it can
    /// scroll up / down (the ▲▼ are shown and tappable only then).
    context_room: Option<(bool, bool)>,
    /// The word the sentence's scroll belongs to (a new word starts afresh).
    context_for: usize,
    /// The Description button on a picture, while it is shown.
    caption_button: Option<Rect>,
}

impl Ui {
    pub fn new(w: u32, h: u32) -> Self {
        Ui { layout: Layout::new(w, h), fonts: Fonts::new(), last: None, pages_key: None, word_box: Rect::default(), page_drawn: None, context_room: None, context_for: usize::MAX, caption_button: None }
    }

    /// Forces a full repaint next time (after wake-up, rotation...).
    pub fn invalidate(&mut self) {
        self.last = None;
        self.page_drawn = None;
    }

    /// The touch target under a point, in screen coordinates.
    pub fn hit(&self, app: &App, x: i32, y: i32) -> Option<Hit> {
        if let (Some(b), Screen::Reading, false) = (self.caption_button, app.screen, app.reader.is_playing()) {
            if b.inset(-6, -6).contains(x, y) {
                return Some(Hit::Caption);
            }
        }
        if let (Some((up, down)), Screen::Reading, false) = (self.context_room, app.screen, app.reader.is_playing()) {
            let [a, b] = layout::context_arrows(&self.layout);
            if a.contains(x, y) {
                return Some(Hit::ContextScroll { down: false, room: up });
            }
            if b.contains(x, y) {
                return Some(Hit::ContextScroll { down: true, room: down });
            }
        }
        hit_test(app, &self.layout, x, y)
    }

    /// Paints what changed and returns the rectangles to send to the panel.
    pub fn paint(&mut self, app: &mut App, now: u64, cv: &mut Canvas) -> Vec<Rect> {
        if app.screen == Screen::Page || app.settings.mode() == Mode::Page {
            self.paginate(app);
        }
        // Past the counted pages: lay out the page on screen so a page turn
        // knows where it ends.
        app.page_hint = None;
        if app.screen == Screen::Page && app.page_index().is_none() {
            let th = theme(app.settings.theme);
            let start = page_start(app);
            let (_, next) = layout_page(&mut self.fonts, app, page_area(th, &self.layout), start, &self.layout);
            app.page_hint = Some((start as u32, next as u32));
        }
        if app.reader.pos() != self.context_for {
            self.context_for = app.reader.pos();
            app.context_scroll = 0;
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
                // A picture shows its Description button only while paused.
                paused && app.reader.on_picture(),
            ),
            context: (paused || matches!(app.screen, Screen::Line { .. })).then_some((app.reader.pos(), app.context_scroll)),
            status: (progress_px(app, &l), app.reader.ms_left() / 60_000, app.page_index(), app.reader.book.chapter_index(app.reader.pos()), paused),
            toolbar: (app.reader.is_playing(), lit, app.reader.wpm(), app.page_index().map(|i| (i, app.pages_done.then_some(app.pages.len())))),
            corner: place_bookmarked(app),
        };
        let old = self.last.take();
        // A bookmark appearing or going is rare: repaint all, simply.
        let full = old.as_ref().is_none_or(|o| o.look != view.look || o.frame != view.frame || o.corner != view.corner);
        let f = &mut self.fonts;
        let word_box = &mut self.word_box;
        let mut damage = Vec::new();
        let ctx = Ctx { th, l: &l, app, now };
        let mut room: Option<Option<(bool, bool, i16)>> = None;

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
                room = Some(draw_context(f, cv, &ctx));
            }
            draw_status(f, cv, &ctx);
            draw_toolbar(f, cv, &ctx);
            draw_corner(cv, &ctx, view.corner);
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
                    // A redrawn first line may have run under the corner.
                    draw_corner(cv, &ctx, view.corner);
                    damage.push(bookmark_corner(app, &l).rect);
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
                    // A picture covers the sentence area too: push its whole box
                    // (not just the word area), or its lower half never reaches
                    // the panel and lingers under the next words.
                    damage.push(word_box.union(&new_box).intersect(&l.stage));
                    *word_box = new_box;
                }
                if o.context != view.context {
                    room = Some(draw_context(f, cv, &ctx));
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
        let mut view = view;
        if let Some(r) = room {
            // Keep the scroll in range, and remember whether the arrows are live.
            self.context_room = r.map(|(up, down, _)| (up, down));
            if let Some((_, _, scroll)) = r {
                app.context_scroll = scroll;
            }
            view.context = view.context.map(|(p, _)| (p, app.context_scroll));
        }
        if app.screen != Screen::Reading || app.reader.is_playing() {
            self.context_room = None;
        }
        // The Description button is live only on a paused picture that has one.
        self.caption_button = None;
        if app.screen == Screen::Reading && !app.reader.is_playing() && app.reader.on_picture() && app.reader.book.caption(app.reader.pos()).is_some() {
            let p = paper(th, &l);
            let area = Rect::new(l.word.x, l.word.y, l.word.w, l.context.bottom() - l.word.y).intersect(&p);
            self.caption_button = Some(caption_button_rect(&l, area, &mut self.fonts, th));
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

/// A word's text for running text (ticker, context, line view): pictures
/// show as a short placeholder there.
fn running_text(book: &reader_core::Book, w: reader_core::Word) -> &str {
    if book.is_image(w) { "[figure]" } else { book.text_of(w) }
}

/// "3 sentences forward", "1 page back", or "Back where you started".
fn moved(net: i16, one: &str, many: &str, per: i32) -> String {
    let n = net.unsigned_abs() as i32 * per;
    if n == 0 {
        return "Back where you started".into();
    }
    format!("{n} {} {}", if n == 1 { one } else { many }, if net > 0 { "forward" } else { "back" })
}

/// Whether the place on screen has a bookmark: the word(s) shown, or in
/// page view anything on the page.
fn place_bookmarked(app: &App) -> bool {
    match app.screen {
        Screen::Page => {
            let start = page_start(app);
            let end = app.page_index().and_then(|i| app.pages.get(i + 1)).map_or(app.reader.shown().end, |&e| e as usize);
            app.bookmarked(start..end.max(start + 1))
        }
        Screen::Reading | Screen::Line { .. } => app.bookmarked(app.reader.shown()),
        _ => false,
    }
}

/// The page's top-right corner. Bookmarked: dog-eared, the flap folded down
/// over the page with a soft shadow. Otherwise a faint fold line hints that
/// the corner can be folded.
fn draw_corner(cv: &mut Canvas, c: &Ctx, marked: bool) {
    let (th, l, app) = (c.th, c.l, c.app);
    if !matches!(app.screen, Screen::Reading | Screen::Page | Screen::Line { .. }) {
        return;
    }
    let r = bookmark_corner(app, l).rect;
    let (x0, y0, s) = (r.x as f32, r.y as f32, r.w as f32);
    if !marked {
        cv.line((x0 + s * 0.35, y0 + 1.0), (x0 + s - 1.0, y0 + s * 0.65), l.px(1.0), mix(th.page, th.text, 45));
        return;
    }
    if th.chrome == Chrome::Win98 {
        draw_corner_98(cv, th, r);
        return;
    }
    // What lies behind the page shows where the corner was.
    let behind = match th.chrome {
        Chrome::Scrivener => th.desktop[0].1,
        _ => mix(th.page, BLACK, 95),
    };
    cv.polygon(&[(x0, y0), (x0 + s, y0), (x0 + s, y0 + s)], behind);
    // Soft shadow under the flap, then the flap itself (the paper's back side).
    cv.clip(Rect::new(r.x - l.i(10.0), r.y, r.w + l.i(10.0), r.h + l.i(10.0)));
    cv.glow(x0 + s * 0.3, y0 + s * 0.78, s * 0.8, s * 0.55, BLACK, 110);
    cv.unclip();
    let flap = if th.chrome == Chrome::Ink { th.page } else { mix(th.page, BLACK, 38) };
    cv.polygon(&[(x0, y0), (x0 + s, y0 + s), (x0, y0 + s)], flap);
    // The crease catches the light.
    cv.line((x0 + 0.5, y0 + 0.5), (x0 + s - 0.5, y0 + s - 0.5), l.px(1.0), mix(th.page, WHITE, 120));
    if th.chrome == Chrome::Ink {
        cv.line((x0, y0), (x0, y0 + s), 1.0, th.text);
        cv.line((x0, y0 + s), (x0 + s, y0 + s), 1.0, th.text);
    }
}

/// The dog-ear the Windows 98 way: whole pixels, no blur. The desktop teal
/// shows through the cut, the flap is button grey with the classic bevel
/// (white on the lit fold, dark grey and black on the edges away from the
/// light) and its shadow is a hard 50% checkerboard, as 98 drew shadows.
fn draw_corner_98(cv: &mut Canvas, th: &Theme, r: Rect) {
    let n = r.w;
    let (x0, y0) = (r.x, r.y);
    let px = |cv: &mut Canvas, x: i32, y: i32, c: Rgb| cv.fill(Rect::new(x, y, 1, 1), c);
    let face = 0xC0C0C0;
    for j in 0..n {
        for i in 0..n {
            let (x, y) = (x0 + i, y0 + j);
            if i > j {
                px(cv, x, y, th.desktop[0].1); // cut away: the desktop behind
            } else {
                px(cv, x, y, face); // the flap
            }
        }
    }
    // Shadow: dithered, one pixel down-left of the flap's outer edges.
    for k in 0..n + 2 {
        for (x, y) in [(x0 - 2, y0 + k), (x0 - 1 + k - 2, y0 + n + 1), (x0 - 1, y0 + k + 1), (x0 + k - 2, y0 + n)] {
            if (x + y) % 2 == 0 && y > y0 && x < x0 + n {
                px(cv, x, y, 0x000000);
            }
        }
    }
    // Bevel: the fold catches the light; the outer edges are in shade.
    for k in 0..n {
        px(cv, x0 + k, y0 + k, 0xFFFFFF);
        if k + 1 < n {
            px(cv, x0 + k + 1, y0 + k, 0x808080);
        }
        px(cv, x0, y0 + k, 0x808080);
        px(cv, x0 + k, y0 + n - 1, 0x000000);
        px(cv, x0 - 1, y0 + k, 0x000000);
    }
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
    // The four menu buttons, top-left: Settings, Book, Library, Text.
    let hb = header_buttons(l);
    for b in &hb {
        draw_icon_button(cv, th, l, b, app.lit(c.now) == Some(b.hit));
    }
    let x = hb[3].rect.right() as f32 + l.px(8.0);
    let base = r.cy() + l.px(4.5);
    // Battery, top-right; a message showing takes its place for a moment.
    let mut right = r.w as f32 - l.px(8.0);
    let small = l.px(11.0);
    match app.battery {
        _ if app.toast().is_some() => {}
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
    let shown = app.reader.shown();
    if mode != Mode::Ticker && shown.len() == 1 && book.is_image(book.word(shown.start).unwrap()) {
        // A picture: it gets the whole stage (word and context areas).
        let area = Rect::new(l.word.x, l.word.y, l.word.w, l.context.bottom() - l.word.y).intersect(&p);
        clear(cv, th, l, area);
        match book.image_of(book.word(shown.start).unwrap()) {
            Some(img) => {
                cv.image(area.inset(l.i(10.0), l.i(4.0)), img.w, img.h, &img.px);
                if !app.reader.is_playing() {
                    if let Some(text) = book.caption(shown.start) {
                        draw_caption(f, cv, c, area, text);
                    }
                }
            }
            None => {
                let msg = up(th, "[figure missing]");
                let w = f.measure(th.ui.0, l.px(13.0), &msg);
                f.draw(cv, th.ui.0, l.px(13.0), &msg, area.cx() - w / 2.0, area.cy(), th.dim);
            }
        }
        return area;
    }
    if mode == Mode::Ticker {
        // Only the strip the line of text occupies is redrawn each frame.
        let band = ticker_band(f, c).intersect(&l.word.intersect(&p));
        clear(cv, th, l, band);
        draw_ticker(f, cv, c);
        return band;
    }
    let fam = family(app.settings.font);
    // Parentheses as side marks: brackets leave the words and sit at the
    // screen's edges while the reader is inside ( … ).
    let side_marks = app.settings.brackets && app.reader.shown().any(|i| book.word(i).is_some_and(|w| w.style.bracketed()));
    let mut words: Vec<(String, Face, bool)> = app
        .reader
        .shown()
        .map(|i| {
            let w = book.word(i).unwrap();
            let t = book.text_of(w);
            let t = if side_marks { t.replace(reader_core::book::OPENERS, "").replace(reader_core::book::CLOSERS, "") } else { String::from(t) };
            (t, fam.face(w.style), w.style.heading())
        })
        .filter(|(t, _, _)| !t.is_empty())
        .collect();
    if words.is_empty() {
        return l.word.intersect(&p);
    }
    // A long word in two flashes: show the half that is due.
    if let (Some(second), 1) = (app.reader.split_part(), words.len()) {
        if let Some((a, b)) = reader_core::book::split_long(&words[0].0) {
            words[0].0 = if second { b } else { a };
        }
    }
    let heading = words.iter().any(|w| w.2);
    // One size per mode, never changing from flash to flash (Spritz,
    // OpenSpritz and Jetzt never resize; resizing reads as jumping).
    let phrase = mode == Mode::Phrase;
    let mut px = word_px(c) * if phrase { PHRASE_SCALE } else { 1.0 };
    let measure = |f: &mut Fonts, px: f32| -> f32 {
        let space = f.measure(words[0].1, px, " ");
        words.iter().map(|(s, face, _)| f.measure(*face, px, s)).sum::<f32>() + space * (words.len() - 1) as f32
    };
    let mut total = measure(f, px);
    // Side marks take the edges; words stay clear of them.
    let margin = if side_marks { word_px(c) * 0.75 } else { l.px(8.0) };
    let room = p.w as f32 - 2.0 * margin;
    if total > room {
        // Only a word or group wider than the whole page shrinks.
        px *= room / total;
        total = measure(f, px);
    }
    let fix = fixation_x(p);
    let x0 = if app.settings.position == 0 {
        // Centred: the word or group in the middle of the page.
        p.cx() - total / 2.0
    } else if !phrase && words.len() == 1 {
        // A single word: its optimal recognition letter sits on the fixation
        // point (Spritz patent US9483109: 2nd letter of 2–5, 3rd of 6–9,
        // 4th of 10–13), so the eye never moves.
        let (s, face, _) = &words[0];
        fix - recognition_x(f, *face, px, s)
    } else {
        // A group starts a fixed few letters before the fixation point: the
        // eye takes in ~4 letters left of where it rests and ~15 right
        // (McConkie & Rayner), so the group's start never jumps (the
        // original e-reader's reason for a fixed left edge).
        fix - f.measure(words[0].1, px, "abcd") * 0.875
    };
    // Rarely a long group runs past the right edge: it starts further left.
    let x0 = x0.min(p.right() as f32 - margin - total).max(p.x as f32 + margin);
    let base = word_baseline(f, c);
    let m = f.line(fam.face(Style::PLAIN), px);
    let colour = if heading { th.heading } else { th.text };
    let mut x = x0;
    cv.clip(l.word.intersect(&p));
    let space = f.measure(words[0].1, px, " ");
    for (s, face, _) in &words {
        x += f.draw(cv, *face, px, s, x, base, colour) + space;
    }
    if side_marks {
        let face = fam.face(Style::PLAIN);
        let bpx = word_px(c) * 1.15;
        let inset = l.px(5.0);
        let (open, close) = reader_core::book::bracket_pair(book, app.reader.pos());
        f.draw(cv, face, bpx, open, p.x as f32 + inset, base, th.dim);
        let rw = f.measure(face, bpx, close);
        f.draw(cv, face, bpx, close, p.right() as f32 - inset - rw, base, th.dim);
    }
    // Chapter titles: bold (from the markup) and the heading colour. No ornaments.
    cv.unclip();
    let top = base - m.ascent - l.px(3.0);
    let pad = l.px(3.0);
    if side_marks {
        // The marks sit at the edges: the whole strip is this frame's box.
        let tall = word_px(c) * 1.15;
        return Rect::new(p.x, (base - tall) as i32, p.w, (tall * 1.4) as i32).intersect(&l.word.intersect(&p));
    }
    Rect::new((x0 - pad) as i32, top as i32, (total + 2.0 * pad) as i32 + 1, (m.ascent + m.descent + l.px(6.0)) as i32 + 1)
        .intersect(&l.word.intersect(&p))
}

/// Phrase and ticker text size relative to the single-word size.
const PHRASE_SCALE: f32 = 0.62;

/// Where the Description button sits on a picture (bottom right).
fn caption_button_rect(l: &Layout, area: Rect, f: &mut Fonts, th: &Theme) -> Rect {
    let label_w = f.measure(th.ui.1, l.px(11.0), "Description");
    let (w, h) = ((label_w + l.px(16.0)) as i32, l.i(20.0));
    Rect::new(area.right() - w - l.i(6.0), area.bottom() - h - l.i(4.0), w, h)
}

/// A picture's description over its lower part (when turned on), and the
/// button that shows or hides it.
fn draw_caption(f: &mut Fonts, cv: &mut Canvas, c: &Ctx, area: Rect, text: &str) {
    let (th, l, app) = (c.th, c.l, c.app);
    let pill = caption_button_rect(l, area, f, th);
    if app.settings.captions {
        // Wrap to the full width, at most four lines, on a band of page colour.
        let px = l.px(12.0);
        let face = family(app.settings.font).face(Style::PLAIN);
        let max_w = area.w as f32 - l.px(16.0);
        let space = f.measure(face, px, " ");
        let mut lines: Vec<String> = alloc::vec![String::new()];
        let mut lw = 0.0;
        for word in text.split_whitespace() {
            let word = word.trim_matches('*');
            let ww = f.measure(face, px, word);
            if lw + ww > max_w && !lines.last().unwrap().is_empty() {
                lines.push(String::new());
                lw = 0.0;
            }
            let line = lines.last_mut().unwrap();
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
            lw += ww + space;
        }
        if lines.len() > 4 {
            lines.truncate(4);
            lines[3].push('…');
        }
        let lh = l.px(15.0);
        let band_h = (lh * lines.len() as f32 + l.px(10.0)) as i32 + pill.h + l.i(6.0);
        let band = Rect::new(area.x + l.i(4.0), area.bottom() - band_h - l.i(2.0), area.w - l.i(8.0), band_h);
        // The theme's own panel: Windows 98 gets its pale-yellow tooltip.
        let text_c = if th.chrome == Chrome::Win98 {
            cv.fill(band, BLACK);
            cv.fill(band.inset(1, 1), 0xFFFFE1);
            BLACK
        } else {
            button(cv, th, l, band, Kind::Row, false)
        };
        let (face, tpx) = if th.chrome == Chrome::Win98 { (th.ui.0, l.px(11.5)) } else { (face, px) };
        for (k, line) in lines.iter().enumerate() {
            let y = band.y as f32 + l.px(5.0) + lh * (k as f32 + 0.78);
            f.draw(cv, face, tpx, line, band.x as f32 + l.px(8.0), y, text_c);
        }
    }
    // The button, in the theme's style (pushed in while the description shows).
    let on = app.settings.captions;
    let c = button(cv, th, l, pill, Kind::Row, on);
    let label = up(th, if on { "Hide" } else { "Description" });
    let lw = f.measure(th.ui.1, l.px(11.0), &label);
    let off = if th.chrome == Chrome::Win98 && on { 1.0 } else { 0.0 };
    f.draw(cv, th.ui.1, l.px(11.0), &label, pill.cx() - lw / 2.0 + off, pill.cy() + l.px(4.0) + off, c);
}

/// Distance from a word's start to the middle of its recognition letter.
/// Measured with the letter (and the one before it) together, so kerning
/// between them is included.
pub fn recognition_x(f: &mut Fonts, face: Face, px: f32, s: &str) -> f32 {
    let k = reader_core::book::orp_index(s);
    let upto = s.char_indices().nth(k).map_or(s.len(), |(i, _)| i);
    let through = s.char_indices().nth(k + 1).map_or(s.len(), |(i, _)| i);
    (f.measure(face, px, &s[..upto]) + f.measure(face, px, &s[..through])) / 2.0
}

/// Where the eye rests in Word and Phrase modes: a little left of centre,
/// as the Spritz reticle, so long words and groups have room on the right.
pub fn fixation_x(p: Rect) -> f32 {
    p.x as f32 + p.w as f32 * 0.37
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
    let cw = f.measure(fam.face(cur.style), px, running_text(book, cur));
    // Typewriter: the line jumps one word per step and stands still in
    // between (a glide is tiring for the eye). The new word types in at the
    // anchor; the words read move to the left in steps.
    let _ = cw;
    // With the recognition point, the new word's letter lands on the same
    // fixation point as in Word mode; otherwise words start at the anchor.
    let x_cur = if app.settings.position == 1 {
        fixation_x(p) - recognition_x(f, fam.face(cur.style), px, running_text(book, cur))
    } else {
        p.x as f32 + p.w as f32 * 0.32
    };
    cv.clip(l.word.intersect(&p));
    let colour = |w: reader_core::Word, past: bool| {
        if past { th.dim } else if w.style.heading() { th.heading } else { th.text }
    };
    let mut x = x_cur;
    let mut i = pos;
    while x < p.right() as f32 && i < book.len() {
        let w = word(i);
        x += f.draw(cv, fam.face(w.style), px, running_text(book, w), x, base, colour(w, false)) + space;
        i += 1;
    }
    let mut x = x_cur - space;
    let mut i = pos;
    while x > p.x as f32 && i > 0 {
        i -= 1;
        let w = word(i);
        let s = running_text(book, w);
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
    let cur_s = running_text(book, cur);
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
        x += f.draw(cv, fam.face(w.style), px, running_text(book, w), x, base, th.text) + space;
        i += 1;
    }
    let mut x = x0 - space;
    let mut i = pos;
    while x > p.x as f32 && i > 0 {
        i -= 1;
        let w = word(i);
        let s = running_text(book, w);
        let ww = f.measure(fam.face(w.style), px, s);
        f.draw(cv, fam.face(w.style), px, s, x - ww, base, th.dim);
        x -= ww + space;
    }
    cv.unclip();
}

/// Under the word, only while paused: the sentence around it (RSVP's known
/// weakness is that you can't glance back; this restores the glance).
/// The sentence under the word while paused. When it is longer than its two
/// lines, the two lines follow the current word and the ▲▼ beside it scroll
/// through the rest. Returns (can scroll up, can scroll down, scroll in range).
fn draw_context(f: &mut Fonts, cv: &mut Canvas, c: &Ctx) -> Option<(bool, bool, i16)> {
    let (th, l, app) = (c.th, c.l, c.app);
    let on_picture = matches!(app.screen, Screen::Reading)
        && app.settings.mode() != Mode::Ticker
        && app.reader.shown().len() == 1
        && app.reader.book.word(app.reader.pos()).is_some_and(|w| app.reader.book.is_image(w));
    if on_picture {
        return None; // the picture covers this area
    }
    clear(cv, th, l, l.context);
    let p = paper(th, l);
    let r = l.context.intersect(&p);
    if let Screen::Line { .. } = app.screen {
        let s = up(th, "Release to keep reading from here");
        let w = f.measure(th.ui.0, l.px(11.0), &s);
        f.draw(cv, th.ui.0, l.px(11.0), &s, r.cx() - w / 2.0, r.cy() + l.px(4.0), th.dim);
        return None;
    }
    if app.screen != Screen::Reading || app.reader.is_playing() || app.reader.book.is_empty() || !app.settings.context {
        return None;
    }
    if app.reader.shown().len() == 1 && app.reader.book.word(app.reader.pos()).is_some_and(|w| app.reader.book.is_image(w)) {
        return None; // a picture fills the stage; no sentence under it
    }
    let book = &app.reader.book;
    let fam = family(app.settings.font);
    let px = l.px(12.5);
    let face = fam.face(Style::PLAIN);
    let space = f.measure(face, px, " ");
    let shown = app.reader.shown();
    let start = book.sentence_start(shown.start);
    // The sentence uses the whole width, edge to edge; when it needs the
    // ▲▼ (more than two lines), it leaves their column free on the right.
    let edge = l.px(8.0);
    let arrows = l.px(30.0);
    let wrap = |f: &mut Fonts, max_w: f32| -> Vec<Vec<(usize, f32)>> {
        let mut lines: Vec<Vec<(usize, f32)>> = alloc::vec![Vec::new()];
        let mut lw = 0.0;
        let mut i = start;
        while i < book.len() {
            let w = book.word(i).unwrap();
            let ww = f.measure(fam.face(w.style), px, running_text(book, w));
            if lw + ww > max_w && !lines.last().unwrap().is_empty() {
                if lines.len() == 40 {
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
        lines
    };
    let mut lines = wrap(f, r.w as f32 - 2.0 * edge);
    let mut area = (r.x as f32 + edge, r.right() as f32 - edge);
    if lines.len() > 2 {
        lines = wrap(f, r.w as f32 - edge - arrows);
        area.1 = r.right() as f32 - arrows;
    }
    // Two lines at a time: the current word's line first, then as scrolled.
    const ROWS: usize = 2;
    let here = lines.iter().position(|ln| ln.iter().any(|&(i, _)| i >= shown.start)).unwrap_or(0);
    let last_first = lines.len().saturating_sub(ROWS) as i16;
    let base = (here as i16).min(last_first);
    let first = (base + app.context_scroll).clamp(0, last_first);
    let visible = &lines[first as usize..(first as usize + ROWS).min(lines.len())];
    let lh = l.px(15.0);
    let top = r.y as f32 + (r.h as f32 - lh * visible.len() as f32) / 2.0 + lh * 0.78;
    let (up, down) = (first > 0, (first as usize) + ROWS < lines.len());
    if lines.len() > ROWS {
        // ▲▼, dimmed where there is nothing more.
        let [a, b] = layout::context_arrows(l);
        for (rect, icon, on) in [(a, Icon::Up, up), (b, Icon::Down, down)] {
            let col = if on { th.text } else { mix(th.page, th.dim, 90) };
            crate::icons::draw(cv, icon, rect.cx(), rect.cy(), l.px(12.0), col);
        }
    }
    for (n, line) in visible.iter().enumerate() {
        let width: f32 = line.iter().map(|w| w.1).sum::<f32>() + space * (line.len().saturating_sub(1)) as f32;
        let x0 = (area.0 + area.1) / 2.0 - width / 2.0;
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
            f.draw(cv, fam.face(w.style), px, running_text(book, w), x, y, if here { th.mark_text } else { th.dim });
            x += ww + space;
        }
    }
    (lines.len() > ROWS).then_some((up, down, first - base))
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
        // Jumps say where you are from where you started: "3 sentences forward".
        Msg::Skipped { net, skip } => match skip {
            Skip::Sentence => moved(net, "sentence", "sentences", 1),
            Skip::Paragraph => moved(net, "paragraph", "paragraphs", 1),
            Skip::Words(n) => moved(net, "word", "words", n as i32),
        },
        Msg::Turned { net } => moved(net, "page", "pages", 1),
        Msg::Stepped { net } => moved(net, "word", "words", 1),
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
        Msg::BookAdded => "Book added to the library".into(),
        Msg::BookRemoved => "Book deleted".into(),
        Msg::Bookmarked => "Bookmarked".into(),
        Msg::NoCard => "No SD card in the reader".into(),
        Msg::NoBooks => "No books to delete".into(),
        Msg::Deleting => "Deleting…".into(),
        Msg::CantDelete => "This book can't be deleted".into(),
        Msg::PictureHeld => "Picture: press play to go on".into(),
        Msg::CaptionShown => "Description shown".into(),
        Msg::CaptionHidden => "Description hidden".into(),
        Msg::Formatting => "Formatting the card…".into(),
        Msg::CardReady => "SD card ready".into(),
        Msg::CardFailed => "Could not format the card".into(),
        Msg::BookmarkRemoved => "Bookmark removed".into(),
        Msg::NoBookmarks => "No bookmarks yet: tap the page corner".into(),
        Msg::BooksRemoved => "Uploaded books removed".into(),
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
    /// A picture's drawn height (0 for words); `base` is then its bottom.
    img_h: f32,
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
        if book.is_image(w) {
            // A picture is a block of its own, scaled to the column.
            if !flush(&mut line, &mut out, &mut y, line_heading, gap_before, true, f) {
                let next = next_of(&out);
                return (out, next);
            }
            x = 0.0;
            let (iw, ih) = book.image_dims(w).map_or((area.w as f32, base_px * 2.0), |(w, h)| (w as f32, h as f32));
            let scale = (area.w as f32 / iw).min(area.h as f32 / ih).min(2.0);
            let (dw, dh) = (iw * scale, ih * scale);
            let gap = base_px * 0.5;
            if y + gap + dh > area.bottom() as f32 && !out.is_empty() {
                return (out, i); // the picture starts the next page
            }
            y += gap;
            out.push(Placed { idx: i, x: area.x as f32 + (area.w as f32 - dw) / 2.0, base: y + dh, w: dw, face: fam.face(w.style), px: base_px, img_h: dh });
            y += dh + gap;
            gap_before = 0.0;
            i += 1;
            continue;
        }
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
        line.push(Placed { idx: i, x: area.x as f32 + x, base: 0.0, w: ww, face, px, img_h: 0.0 });
        x += ww + space;
        i += 1;
        if w.pause == Pause::Paragraph {
            if !flush(&mut line, &mut out, &mut y, line_heading, gap_before, true, f) {
                let next = next_of(&out);
                return (out, next);
            }
            gap_before = if st.indent && !heading { 0.0 } else { base_px * 0.6 };
            x = 0.0;
        } else if w.style.line_end() {
            // Verse: the poem's own line break (never justified).
            if !flush(&mut line, &mut out, &mut y, line_heading, gap_before, true, f) {
                let next = next_of(&out);
                return (out, next);
            }
            gap_before = 0.0;
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
        if p.img_h > 0.0 {
            return Rect::new(text_area.x, (p.base - p.img_h) as i32 - 1, text_area.w, p.img_h as i32 + 2);
        }
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
            let lit: Vec<usize> = (a..=z).filter(|&k| placed[k].img_h == 0.0 && phrase.contains(&placed[k].idx)).collect();
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
            if p.img_h > 0.0 {
                let r = Rect::new(p.x as i32, (p.base - p.img_h) as i32, p.w as i32 + 1, p.img_h as i32 + 1);
                match book.image_of(w) {
                    Some(img) => {
                        cv.image(r, img.w, img.h, &img.px);
                    }
                    None => {
                        cv.rrect_stroke(r, 2.0, th.dim);
                    }
                }
                if show && phrase.contains(&p.idx) {
                    // The picture being "read": a frame instead of a highlight.
                    cv.rrect_stroke(Rect::new(r.x - 2, r.y - 2, r.w + 4, r.h + 4), 3.0, th.fill);
                }
                continue;
            }
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

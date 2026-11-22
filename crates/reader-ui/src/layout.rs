//! Where everything goes, computed from the panel size, plus touch hit
//! testing. Buttons are described once here and used both for drawing and
//! for hits, so what you see is exactly what you can tap.

#[allow(unused_imports)]
use core_maths::CoreFloat;
use alloc::string::String;
use alloc::vec::Vec;

use reader_core::app::{menu_item, MenuItem, PickKind, MENU};
use reader_core::{App, Hit, Key, Screen};

use crate::geom::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub screen: Rect,
    pub header: Rect,
    pub stage: Rect,
    pub toolbar: Rect,
    /// Where messages appear: the title bar, on every screen.
    pub toast: Rect,
    /// Inside the stage: the word.
    pub word: Rect,
    /// Inside the stage: sentence context when paused.
    pub context: Rect,
    /// Inside the stage: progress line.
    pub status: Rect,
    /// Scale relative to a 320×240 design.
    pub s: f32,
}

impl Layout {
    pub fn new(w: u32, h: u32) -> Self {
        let (w, h) = (w as i32, h as i32);
        let s = (w as f32 / 320.0).min(h as f32 / 240.0).max(0.5);
        let i = |v: f32| (v * s).round() as i32;
        let header = Rect::new(0, 0, w, i(24.0));
        let toolbar = Rect::new(0, h - i(48.0), w, i(48.0));
        let stage = Rect::new(0, header.bottom(), w, toolbar.y - header.bottom());
        let status = Rect::new(0, stage.bottom() - i(26.0), w, i(26.0));
        let toast = header;
        let context = Rect::new(0, status.y - i(36.0), w, i(36.0));
        let word = Rect::new(0, stage.y + i(6.0), w, (context.y - stage.y - i(6.0)).max(1));
        Layout { screen: Rect::new(0, 0, w, h), header, stage, toolbar, toast, word, context, status, s }
    }

    pub fn px(&self, v: f32) -> f32 {
        v * self.s
    }

    pub fn i(&self, v: f32) -> i32 {
        (v * self.s).round() as i32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Menu,
    Gear,
    Book,
    Library,
    Text,
    SentenceBack,
    SentenceForward,
    Play,
    Pause,
    Minus,
    Plus,
    PageBack,
    PageForward,
    Bolt,
    Back,
    Close,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    /// What is drawn.
    pub rect: Rect,
    /// What responds to touch (bigger than the drawing, never overlapping).
    pub hit_rect: Rect,
    pub hit: Hit,
    pub icon: Icon,
    /// The main action (play): drawn in the primary style.
    pub primary: bool,
}

/// A text label in the toolbar (speed, page number). Not tappable.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub rect: Rect,
    pub big: String,
    pub small: String,
}

/// The paper the text sits on: the whole reading area, edge to edge, in
/// every theme (no side bands; the screen is small).
pub fn paper(_th: &crate::theme::Theme, l: &Layout) -> Rect {
    l.stage
}

/// The page's top-right corner: tap to dog-ear (bookmark) the current place.
/// `rect` is the drawn triangle's square; the touch area is larger.
pub fn bookmark_corner(app: &App, l: &Layout) -> Button {
    // Right in the paper's corner, small, so the page stays free.
    let p = paper(crate::themes::theme(app.settings.theme), l);
    let size = l.i(28.0);
    let rect = Rect::new(p.right() - size, p.y, size, size);
    Button { rect, hit_rect: Rect::new(p.right() - l.i(36.0), p.y, l.i(36.0), l.i(32.0)), hit: Hit::Bookmark, icon: Icon::Menu, primary: false }
}

/// The small ▲ and ▼ at the right of the sentence under the word.
pub fn context_arrows(l: &Layout) -> [Rect; 2] {
    let r = l.context;
    let (w, h) = (l.i(26.0), r.h / 2);
    [Rect::new(r.right() - w - l.i(2.0), r.y, w, h), Rect::new(r.right() - w - l.i(2.0), r.y + h, w, r.h - h)]
}

/// The four menu buttons, left in the title bar: Settings, Book, Library,
/// Text (see `reader_core::app::MENU`). Touch areas meet edge to edge and
/// reach a little below the bar, easier for a thumb.
pub fn header_buttons(l: &Layout) -> [Button; 4] {
    let h = l.header;
    let (w, gap) = (l.i(32.0), l.i(4.0));
    let icons = [Icon::Gear, Icon::Book, Icon::Library, Icon::Text];
    core::array::from_fn(|k| {
        let x = l.i(4.0) + k as i32 * (w + gap);
        let rect = Rect::new(x, h.y + l.i(3.0), w, h.h - l.i(6.0));
        let hx = if k == 0 { 0 } else { x - gap / 2 };
        let hit_rect = Rect::new(hx, h.y, x + w + gap / 2 - hx, h.h + l.i(12.0));
        Button { rect, hit_rect, hit: Hit::Group(k as u8), icon: icons[k], primary: false }
    })
}

/// The small highlight dot, top-right in Page mode (left of the battery).
pub fn highlight_button(l: &Layout) -> Button {
    let h = l.header;
    let size = l.i(18.0);
    let cx = h.right() - l.i(78.0);
    let rect = Rect::new(cx - size / 2, h.cy() as i32 - size / 2, size, size);
    Button { rect, hit_rect: Rect::new(cx - l.i(22.0), h.y, l.i(44.0), h.h), hit: Hit::Highlight, icon: Icon::Menu, primary: false }
}

/// Bottom bar: speed on the left (− value +), then the transport in
/// music-player order (⏮ ▶ ⏭). Page view: page number, then ◀ ⚡ ▶.
pub fn toolbar(app: &App, l: &Layout) -> (Vec<Button>, Option<Label>) {
    let t = l.toolbar;
    let bh = l.i(34.0);
    let cy = t.cy() as i32;
    let at = |x: i32, w: i32| Rect::new(x, cy - bh / 2, w, bh);
    let small = l.i(38.0);
    let play_w = l.i(58.0);
    let gap = l.i(5.0);
    let pad = l.i(6.0);
    let key = |k: Key| Hit::Key(k);
    let mut v: Vec<(Rect, Hit, Icon, bool)> = Vec::new();

    // Transport, right-aligned.
    let x2 = t.right() - pad - small;
    let x1 = x2 - gap - play_w;
    let x0 = x1 - gap - small;
    // Same bar in every mode: speed on the left, transport on the right.
    // In Page mode ⏮/⏭ turn pages, so they show page arrows.
    let page = app.screen == Screen::Page;
    let play = if app.reader.is_playing() { Icon::Pause } else { Icon::Play };
    let (back, fwd) = if page { (Icon::PageBack, Icon::PageForward) } else { (Icon::SentenceBack, Icon::SentenceForward) };
    v.push((at(x0, small), key(Key::Left), back, false));
    v.push((at(x1, play_w), key(Key::Select), play, true));
    v.push((at(x2, small), key(Key::Right), fwd, false));
    let sw = l.i(34.0);
    let lw = l.i(46.0);
    v.push((at(pad, sw), key(Key::Down), Icon::Minus, false));
    v.push((at(pad + sw + lw, sw), key(Key::Up), Icon::Plus, false));
    let label = Some(Label {
        rect: Rect::new(pad + sw, t.y, lw, t.h),
        big: alloc::format!("{}", app.reader.wpm()),
        small: String::from("wpm"),
    });
    v.sort_by_key(|e| e.0.x);
    // Hit areas: full bar height, split halfway between neighbours (Fitts),
    // never over the label between − and +.
    let mut out = Vec::new();
    for (i, &(rect, hit, icon, primary)) in v.iter().enumerate() {
        let mut lo = if i == 0 { t.x } else { (v[i - 1].0.right() + rect.x) / 2 };
        let mut hi = if i + 1 == v.len() { t.right() } else { (rect.right() + v[i + 1].0.x) / 2 };
        if let Some(lb) = &label {
            if rect.right() <= lb.rect.x && hi > lb.rect.x {
                hi = lb.rect.x;
            }
            if rect.x >= lb.rect.right() && lo < lb.rect.right() {
                lo = lb.rect.right();
            }
        }
        out.push(Button { rect, hit_rect: Rect::new(lo, t.y, hi - lo, t.h), hit, icon, primary });
    }
    (out, label)
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Section(&'static str),
    /// A tappable row: index into the menu or pick list.
    Item(u8),
    /// Read-only key/value (device info).
    Info(String, String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub rect: Rect,
    pub kind: RowKind,
}

pub struct ListGeom {
    pub header: Rect,
    pub title: String,
    pub back: Button,
    pub close: Button,
    pub area: Rect,
    pub rows: Vec<Row>,
    pub scroll: Option<(Button, Button)>,
    /// Fraction of the list scrolled, for the position indicator.
    pub thumb: Option<(f32, f32)>,
}

/// Every entry in the list for an overlay screen, before scrolling.
fn entries(app: &App) -> (String, Vec<RowKind>, Option<u8>) {
    match app.screen {
        Screen::Menu { sel } => {
            // Only the menu the selection is in (one per title-bar button).
            let (g, range) = reader_core::app::menu_group(sel);
            (String::from(MENU[g].0), range.map(RowKind::Item).collect(), Some(sel))
        }
        Screen::Pick { kind, sel, .. } => {
            let v = (0..app.pick_len(kind)).map(RowKind::Item).collect();
            (String::from(kind.title()), v, Some(sel))
        }
        Screen::Info => {
            let b = &app.reader.book;
            let bat = match app.battery {
                Some(bt) => alloc::format!("{}%  ({}.{:02} V){}", bt.percent, bt.millivolts / 1000, bt.millivolts % 1000 / 10, if bt.charging { ", charging" } else { "" }),
                None => String::from("USB power"),
            };
            let d = &app.device;
            let v = alloc::vec![
                RowKind::Info("Board".into(), d.board.clone()),
                RowKind::Info("Firmware".into(), d.firmware.clone()),
                RowKind::Info("Touch".into(), d.touch.clone()),
                RowKind::Info("Web view".into(), if app.settings.wifi { d.web.clone() } else { "off (Menu → Web view)".into() }),
                RowKind::Info("Battery".into(), bat),
                RowKind::Info("Free memory".into(), alloc::format!("{} KB", d.free_memory_kb)),
                RowKind::Info("Book".into(), alloc::format!("{} words, {} chapters", b.len(), b.chapters().len())),
            ];
            (String::from("Device info"), v, None)
        }
        _ => (String::new(), Vec::new(), None),
    }
}

pub fn list_geom(app: &App, l: &Layout) -> ListGeom {
    let header = Rect::new(0, 0, l.screen.w, l.i(30.0));
    let hb = l.i(24.0);
    let back = Button {
        rect: Rect::new(l.i(4.0), header.cy() as i32 - hb / 2, l.i(36.0), hb),
        hit_rect: Rect::new(0, 0, l.i(64.0), header.h),
        hit: Hit::Key(Key::Left),
        icon: Icon::Back,
        primary: false,
    };
    let close = Button {
        rect: Rect::new(header.right() - l.i(4.0) - l.i(36.0), header.cy() as i32 - hb / 2, l.i(36.0), hb),
        hit_rect: Rect::new(header.right() - l.i(64.0), 0, l.i(64.0), header.h),
        hit: Hit::Key(Key::Menu),
        icon: Icon::Close,
        primary: false,
    };
    let (title, all, sel) = entries(app);
    let full = Rect::new(0, header.bottom(), l.screen.w, l.screen.h - header.bottom());
    // Rows are buttons with a small gap between them.
    let row_h = |k: &RowKind| match k {
        RowKind::Section(_) => l.i(20.0),
        RowKind::Item(_) => l.i(33.0),
        RowKind::Info(..) => l.i(28.0),
    };
    let total: i32 = all.iter().map(row_h).sum();
    let overflow = total > full.h - l.i(4.0);
    let col = l.i(40.0);
    let area = if overflow { Rect::new(0, full.y, full.w - col, full.h) } else { full };

    // Scroll so the selected row sits in the middle (standard list behaviour:
    // you always see what is above and below), clamped at both ends.
    let sel_idx = sel.and_then(|s| all.iter().position(|k| *k == RowKind::Item(s))).unwrap_or(0);
    let mut first = 0;
    if overflow {
        let mut above = 0;
        first = sel_idx;
        while first > 0 && above + row_h(&all[first - 1]) <= (area.h - row_h(&all[sel_idx])) / 2 {
            first -= 1;
            above += row_h(&all[first]);
        }
        // Don't leave empty space at the bottom.
        while first > 0 && all[first..].iter().map(row_h).sum::<i32>() + row_h(&all[first - 1]) <= area.h - l.i(4.0) {
            first -= 1;
        }
    }
    let mut rows = Vec::new();
    let mut y = area.y + l.i(2.0);
    for k in &all[first..] {
        let h = row_h(k);
        if y + h > area.bottom() {
            break;
        }
        let gap = if matches!(k, RowKind::Item(_)) { l.i(4.0) } else { 0 };
        rows.push(Row { rect: Rect::new(area.x + l.i(6.0), y + gap / 2, area.w - l.i(12.0), h - gap), kind: k.clone() });
        y += h;
    }
    let (scroll, thumb) = if overflow {
        let half = full.h / 2;
        let visible_items = rows.iter().filter(|r| matches!(r.kind, RowKind::Item(_))).count() as u8;
        let mk = |down: bool| {
            let hit_rect = Rect::new(full.right() - col, if down { full.y + half } else { full.y }, col, half);
            let bh = l.i(46.0).min(hit_rect.h - l.i(10.0));
            let y = if down { hit_rect.bottom() - l.i(6.0) - bh } else { hit_rect.y + l.i(6.0) };
            Button {
                rect: Rect::new(hit_rect.x + l.i(4.0), y, hit_rect.w - l.i(8.0), bh),
                hit_rect,
                hit: Hit::Scroll { down, rows: visible_items.max(1) },
                icon: if down { Icon::Down } else { Icon::Up },
                primary: false,
            }
        };
        let skipped: i32 = all[..first].iter().map(row_h).sum();
        let shown: i32 = rows.iter().map(|r| r.rect.h).sum();
        (Some((mk(false), mk(true))), Some((skipped as f32 / total as f32, (skipped + shown) as f32 / total as f32)))
    } else {
        (None, None)
    };
    ListGeom { header, title, back, close, area, rows, scroll, thumb }
}

/// Which target is under (x, y), if any.
pub fn hit_test(app: &App, l: &Layout, x: i32, y: i32) -> Option<Hit> {
    if app.screen.is_overlay() {
        let g = list_geom(app, l);
        for b in [&g.back, &g.close] {
            if b.hit_rect.contains(x, y) {
                return Some(b.hit);
            }
        }
        if let Some((up, down)) = &g.scroll {
            for b in [up, down] {
                if b.hit_rect.contains(x, y) {
                    return Some(b.hit);
                }
            }
        }
        return g.rows.iter().find(|r| r.rect.contains(x, y)).and_then(|r| match r.kind {
            RowKind::Item(i) => Some(Hit::Row(i)),
            _ => None,
        });
    }
    if let Some(b) = header_buttons(l).into_iter().find(|b| b.hit_rect.contains(x, y)) {
        return Some(b.hit);
    }
    if app.screen == Screen::Page && highlight_button(l).hit_rect.contains(x, y) {
        return Some(Hit::Highlight);
    }
    if matches!(app.screen, Screen::Reading | Screen::Page) && bookmark_corner(app, l).hit_rect.contains(x, y) {
        return Some(Hit::Bookmark);
    }
    toolbar(app, l).0.into_iter().find(|b| b.hit_rect.contains(x, y)).map(|b| b.hit)
}

/// Label for a menu or pick row.
pub fn row_text(app: &App, i: u8) -> (String, Option<String>, bool) {
    match app.screen {
        Screen::Menu { .. } => {
            let item = menu_item(i);
            (String::from(item.label()), app.menu_value(item), item.pick().is_some() || item == MenuItem::Info)
        }
        Screen::Pick { kind, .. } => (app.pick_label(kind, i), None, false),
        _ => (String::new(), None, false),
    }
}

pub fn is_font_list(app: &App) -> bool {
    matches!(app.screen, Screen::Pick { kind: PickKind::Font, .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_core::{Book, Settings};

    fn app() -> App {
        App::new(Book::new("t", "# A\nOne two three."), crate::CATALOG, Settings::default())
    }

    #[test]
    fn regions_tile_the_screen() {
        for (w, h) in [(320, 240), (240, 320), (480, 320), (800, 480), (296, 160)] {
            let l = Layout::new(w, h);
            assert_eq!(l.header.h + l.stage.h + l.toolbar.h, h as i32, "{w}x{h}");
            assert!(l.word.h > 20, "{w}x{h}: word area {}", l.word.h);
            assert_eq!(l.status.bottom(), l.stage.bottom());
        }
    }

    #[test]
    fn toolbar_hit_areas_never_overlap_and_cover_their_buttons() {
        let mut a = app();
        for screen in [Screen::Reading, Screen::Page] {
            a.screen = screen;
            for (w, h) in [(320, 240), (240, 320), (480, 320)] {
                let l = Layout::new(w, h);
                let (bs, _) = toolbar(&a, &l);
                for (i, b) in bs.iter().enumerate() {
                    assert!(b.hit_rect.contains(b.rect.cx() as i32, b.rect.cy() as i32), "{w}x{h} {:?}", b.icon);
                    assert!(b.hit_rect.w >= l.i(34.0), "{w}x{h} {:?}: target only {} px wide", b.icon, b.hit_rect.w);
                    for o in &bs[i + 1..] {
                        assert!(b.hit_rect.intersect(&o.hit_rect).is_empty(), "{w}x{h}: {:?} overlaps {:?}", b.icon, o.icon);
                    }
                    assert_eq!(hit_test(&a, &l, b.rect.cx() as i32, b.rect.cy() as i32), Some(b.hit));
                }
            }
        }
    }

    #[test]
    fn every_key_has_an_on_screen_button_while_reading() {
        let a = app();
        let l = Layout::new(320, 240);
        let (bs, _) = toolbar(&a, &l);
        for k in Key::ALL {
            // The menu key's job is done by the four title-bar buttons.
            if k == Key::Menu {
                assert_eq!(header_buttons(&l).len(), 4);
                continue;
            }
            assert!(bs.iter().any(|b| b.hit == Hit::Key(k)), "{k:?} has no button");
        }
    }

    #[test]
    fn selected_row_is_always_visible_and_tappable() {
        let mut a = app();
        let l = Layout::new(320, 240);
        for sel in 0..reader_core::app::MENU_LEN {
            a.screen = Screen::Menu { sel };
            let g = list_geom(&a, &l);
            let row = g.rows.iter().find(|r| r.kind == RowKind::Item(sel)).unwrap_or_else(|| panic!("row {sel} not visible"));
            assert_eq!(hit_test(&a, &l, row.rect.cx() as i32, row.rect.cy() as i32), Some(Hit::Row(sel)));
            let n = reader_core::app::menu_group(sel).1.len();
            assert_eq!(g.scroll.is_some(), n > 6, "scroll buttons only when a menu ({n} rows) doesn't fit");
        }
    }

    #[test]
    fn short_lists_have_no_scroll_buttons() {
        let mut a = app();
        a.screen = Screen::Pick { kind: PickKind::Words, sel: 0, original: 0 };
        assert!(list_geom(&a, &Layout::new(320, 240)).scroll.is_none());
    }
}

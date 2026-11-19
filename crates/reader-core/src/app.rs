//! Screens, the key-binding table and user feedback.
//!
//! Every input, from a physical button, an on-screen button or the web page,
//! becomes a [`Key`] or a tapped list row and goes through this module. Each
//! screen has one binding table; the on-screen buttons are drawn from the
//! same table, so a button always does what its physical key does.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::book::Book;
use crate::input::Key;
use crate::reader::Reader;
use crate::settings::{Mode, Settings, Skip, ALIGN_NAMES, MODE_NAMES, PACE_NAMES, PAREN_NAMES, PARAGRAPH_NAMES, SKIPS, SKIP_NAMES, SPACING_NAMES};

/// How long a toast stays visible.
pub const TOAST_MS: u64 = 1_400;
/// How long an on-screen button stays lit after its key was pressed.
pub const FLASH_MS: u64 = 160;

pub const SPEED_PRESETS: [u16; 13] = [150, 200, 250, 300, 350, 400, 450, 475, 500, 600, 700, 800, 1000];
pub const SIZE_NAMES: [&str; 3] = ["Small", "Normal", "Large"];
pub const CHUNK_NAMES: [&str; 3] = ["1 word", "2 words", "3 words"];
pub const BRIGHTNESS_NAMES: [&str; 5] = ["Dim", "Low", "Medium", "Bright", "Full"];

/// Names of the themes and fonts the UI layer provides.
#[derive(Clone, Copy)]
pub struct Catalog {
    pub themes: &'static [&'static str],
    pub fonts: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Resume,
    Library,
    Mode,
    Speed,
    Pace,
    Words,
    Skip,
    Chapters,
    Bookmarks,
    GoTo,
    Restart,
    Size,
    Font,
    Theme,
    Spacing,
    Paragraphs,
    Align,
    Parens,
    Brightness,
    Rotate,
    Wifi,
    Info,
    Sleep,
}

/// Menu groups, shown as section headings.
pub const MENU: [(&str, &[MenuItem]); 3] = [
    ("Reading", &[MenuItem::Resume, MenuItem::Library, MenuItem::Mode, MenuItem::Speed, MenuItem::Pace, MenuItem::Words, MenuItem::Skip, MenuItem::Chapters, MenuItem::Bookmarks, MenuItem::GoTo, MenuItem::Restart]),
    ("Look", &[MenuItem::Size, MenuItem::Font, MenuItem::Theme, MenuItem::Spacing, MenuItem::Paragraphs, MenuItem::Align, MenuItem::Parens]),
    ("Device", &[MenuItem::Brightness, MenuItem::Rotate, MenuItem::Wifi, MenuItem::Info, MenuItem::Sleep]),
];

pub fn menu_items() -> impl Iterator<Item = MenuItem> {
    MENU.iter().flat_map(|(_, items)| items.iter().copied())
}

pub const MENU_LEN: u8 = 23;

pub fn menu_item(i: u8) -> MenuItem {
    menu_items().nth(i as usize).unwrap_or(MenuItem::Resume)
}

pub fn menu_index(item: MenuItem) -> u8 {
    menu_items().position(|i| i == item).unwrap_or(0) as u8
}

impl MenuItem {
    pub const fn label(self) -> &'static str {
        match self {
            MenuItem::Resume => "Resume",
            MenuItem::Library => "Library",
            MenuItem::Mode => "Reading mode",
            MenuItem::Speed => "Speed",
            MenuItem::Pace => "Speed means",
            MenuItem::Words => "Words per flash",
            MenuItem::Skip => "Skip buttons jump",
            MenuItem::Chapters => "Chapters",
            MenuItem::Bookmarks => "Bookmarks",
            MenuItem::GoTo => "Go to",
            MenuItem::Restart => "Start over",
            MenuItem::Size => "Text size",
            MenuItem::Font => "Font",
            MenuItem::Theme => "Theme",
            MenuItem::Spacing => "Line spacing",
            MenuItem::Paragraphs => "Paragraphs",
            MenuItem::Align => "Text alignment",
            MenuItem::Parens => "Parentheses",
            MenuItem::Brightness => "Brightness",
            MenuItem::Rotate => "Rotate screen",
            MenuItem::Wifi => "Web view (Wi-Fi)",
            MenuItem::Info => "Device info",
            MenuItem::Sleep => "Sleep",
        }
    }

    /// The list this item opens, if any.
    pub const fn pick(self) -> Option<PickKind> {
        Some(match self {
            MenuItem::Speed => PickKind::Speed,
            MenuItem::Pace => PickKind::Pace,
            MenuItem::Library => PickKind::Book,
            MenuItem::Mode => PickKind::Mode,
            MenuItem::Spacing => PickKind::Spacing,
            MenuItem::Paragraphs => PickKind::Paragraphs,
            MenuItem::Align => PickKind::Align,
            MenuItem::Words => PickKind::Words,
            MenuItem::Skip => PickKind::Skip,
            MenuItem::Chapters => PickKind::Chapter,
            MenuItem::Bookmarks => PickKind::Bookmark,
            MenuItem::Parens => PickKind::Parens,
            MenuItem::GoTo => PickKind::GoTo,
            MenuItem::Size => PickKind::Size,
            MenuItem::Font => PickKind::Font,
            MenuItem::Theme => PickKind::Theme,
            MenuItem::Brightness => PickKind::Brightness,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    Book,
    Bookmark,
    Parens,
    Pace,
    Mode,
    Spacing,
    Paragraphs,
    Align,
    Speed,
    Words,
    Skip,
    Chapter,
    GoTo,
    Size,
    Font,
    Theme,
    Brightness,
}

impl PickKind {
    pub const fn title(self) -> &'static str {
        match self {
            PickKind::Book => "Library",
            PickKind::Bookmark => "Bookmarks",
            PickKind::Parens => "Parentheses",
            PickKind::Pace => "Speed means",
            PickKind::Mode => "Reading mode",
            PickKind::Spacing => "Line spacing",
            PickKind::Paragraphs => "Paragraphs",
            PickKind::Align => "Text alignment",
            PickKind::Speed => "Speed",
            PickKind::Words => "Words per flash",
            PickKind::Skip => "Skip buttons jump",
            PickKind::Chapter => "Chapters",
            PickKind::GoTo => "Go to",
            PickKind::Size => "Text size",
            PickKind::Font => "Font",
            PickKind::Theme => "Theme",
            PickKind::Brightness => "Brightness",
        }
    }

    /// Settings that apply while you scroll, so you see before you choose.
    pub const fn live_preview(self) -> bool {
        matches!(self, PickKind::Size | PickKind::Font | PickKind::Theme | PickKind::Brightness | PickKind::Spacing | PickKind::Paragraphs | PickKind::Align | PickKind::Parens)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// One word (or short phrase) at a time.
    Reading,
    /// Hold ◀◀/▶▶ while reading: the text as a scrolling line.
    Line { resume: bool },
    /// Normal page-by-page reading.
    Page,
    Menu { sel: u8 },
    Pick { kind: PickKind, sel: u8, original: u8 },
    Info,
}

impl Screen {
    pub fn is_overlay(self) -> bool {
        matches!(self, Screen::Menu { .. } | Screen::Pick { .. } | Screen::Info)
    }
}

/// What a key does on a given screen. One entry per (screen, key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Faster,
    Slower,
    SkipBack,
    SkipForward,
    WordBack,
    WordForward,
    PhraseBack,
    PhraseForward,
    TogglePlay,
    OpenMenu,
    CloseMenu,
    Prev,
    Next,
    Activate,
    Back,
}

impl Action {
    /// Short label, used for the web page tooltips and the audit table.
    pub const fn label(self) -> &'static str {
        match self {
            Action::Faster => "faster",
            Action::Slower => "slower",
            Action::SkipBack => "skip back",
            Action::SkipForward => "skip forward",
            Action::WordBack => "previous word",
            Action::WordForward => "next word",
            Action::PhraseBack => "previous phrase",
            Action::PhraseForward => "next phrase",
            Action::TogglePlay => "play / pause",
            Action::OpenMenu => "menu",
            Action::CloseMenu => "close menu",
            Action::Prev => "move up",
            Action::Next => "move down",
            Action::Activate => "choose",
            Action::Back => "back",
        }
    }
}

type Table = [(Key, Action); 6];

const READING: Table = [
    (Key::Up, Action::Faster),
    (Key::Down, Action::Slower),
    (Key::Left, Action::SkipBack),
    (Key::Right, Action::SkipForward),
    (Key::Select, Action::TogglePlay),
    (Key::Menu, Action::OpenMenu),
];
const LINE: Table = [
    (Key::Up, Action::Faster),
    (Key::Down, Action::Slower),
    (Key::Left, Action::WordBack),
    (Key::Right, Action::WordForward),
    (Key::Select, Action::TogglePlay),
    (Key::Menu, Action::OpenMenu),
];
// Same keys as reading: speed on ▲▼, play on ●; ◀ ▶ step phrase by phrase
// (the page follows the phrase, turning by itself).
const PAGE: Table = [
    (Key::Up, Action::Faster),
    (Key::Down, Action::Slower),
    (Key::Left, Action::PhraseBack),
    (Key::Right, Action::PhraseForward),
    (Key::Select, Action::TogglePlay),
    (Key::Menu, Action::OpenMenu),
];
const LIST: Table = [
    (Key::Up, Action::Prev),
    (Key::Down, Action::Next),
    (Key::Left, Action::Back),
    (Key::Right, Action::Activate),
    (Key::Select, Action::Activate),
    (Key::Menu, Action::CloseMenu),
];
const INFO: Table = [
    (Key::Up, Action::Prev),
    (Key::Down, Action::Next),
    (Key::Left, Action::Back),
    (Key::Right, Action::Back),
    (Key::Select, Action::Back),
    (Key::Menu, Action::CloseMenu),
];

pub fn bindings(screen: Screen) -> &'static Table {
    match screen {
        Screen::Reading => &READING,
        Screen::Line { .. } => &LINE,
        Screen::Page => &PAGE,
        Screen::Menu { .. } | Screen::Pick { .. } => &LIST,
        Screen::Info => &INFO,
    }
}

pub fn action_for(screen: Screen, key: Key) -> Action {
    bindings(screen).iter().find(|(k, _)| *k == key).map(|(_, a)| *a).unwrap_or(Action::Back)
}

/// A touch target: an on-screen button (same as its key) or a list row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Key(Key),
    Row(u8),
    /// The list's page-up / page-down buttons, with how many rows fit.
    Scroll { down: bool, rows: u8 },
    /// The small dot in Page mode: highlight the current phrase or not.
    Highlight,
    /// The page's top-right corner: bookmark (dog-ear) the current place.
    Bookmark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Something changed.
    Ok,
    /// Nothing changed because a limit was hit (fastest speed, top of list...).
    Limit,
}

/// What to tell the user. The renderer turns this into text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    Playing,
    Paused,
    Speed(u16),
    /// Jumped by the skip setting.
    Skipped { forward: bool, skip: Skip },
    PageBack,
    PageForward,
    AtStart,
    AtEnd,
    Top,
    Bottom,
    Set(PickKind, u8),
    Kept,
    Restarted,
    Finished,
    Sleeping,
    PagesView,
    SpeedView,
    Highlight(bool),
    Wifi(bool),
    BookAdded,
    BooksRemoved,
    Bookmarked,
    BookmarkRemoved,
    NoBookmarks,
    Rotated,
    NoChapters,
    /// The screen itself shows the change (menu opened, highlight moved).
    Shown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feedback {
    pub tone: Tone,
    pub msg: Msg,
}

impl Feedback {
    const fn ok(msg: Msg) -> Self {
        Feedback { tone: Tone::Ok, msg }
    }
    const fn limit(msg: Msg) -> Self {
        Feedback { tone: Tone::Limit, msg }
    }
    pub fn is_toast(&self) -> bool {
        self.msg != Msg::Shown
    }
}

/// Requests for the platform layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    SaveSettings,
    /// Brightness or rotation changed: apply to the hardware (and save).
    Display,
    Sleep,
    /// Load library book N and call [`App::open_book`].
    OpenBook(u8),
    /// Turn the Wi-Fi web view on or off.
    Wifi(bool),
    /// Bookmarks changed: store `App::bookmarks`.
    SaveBookmarks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toast {
    pub feedback: Feedback,
    pub until: u64,
    /// Bumped on every toast so repeating the same message still re-flashes.
    pub serial: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub millivolts: u16,
}

/// Facts for the Device info screen, filled in by the platform.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    pub board: String,
    pub firmware: String,
    pub touch: String,
    pub free_memory_kb: u32,
    /// How to reach the web view, e.g. "Wi-Fi FastReader → 192.168.4.1".
    pub web: String,
}

pub struct App {
    pub reader: Reader,
    pub catalog: Catalog,
    pub settings: Settings,
    pub screen: Screen,
    pub battery: Option<Battery>,
    pub device: DeviceInfo,
    /// First word of every page, filled in by the renderer (it knows the fonts),
    /// a little at a time; `pages_done` once the whole book is measured.
    pub pages: Vec<u32>,
    pub pages_done: bool,
    /// Titles of the books the platform can open, and where each was left.
    pub library: Vec<String>,
    pub positions: Vec<u32>,
    /// Every bookmark as (library book, word), all books together.
    pub bookmarks: Vec<(u8, u32)>,
    toast: Option<Toast>,
    serial: u32,
    /// The key whose on-screen button is lit, and until when.
    flash: Option<(Key, u64)>,
    /// A touch target under a finger that hasn't lifted yet.
    pressed: Option<Hit>,
    resume_after_menu: bool,
    effects: Vec<Effect>,
    /// Time of the last `tick`, for smooth animation (ticker) frame pacing.
    last_tick: u64,
}

impl App {
    pub fn new(book: Book, catalog: Catalog, mut settings: Settings) -> Self {
        settings.theme %= catalog.themes.len().max(1) as u8;
        settings.font %= catalog.fonts.len().max(1) as u8;
        let mut reader = Reader::new(book, settings.wpm);
        reader.set_chunk(settings.chunk);
        reader.set_pacing(settings.pacing);
        reader.set_true_pace(settings.true_pace);
        reader.seek(settings.pos as usize, 0);
        let mut app = App {
            reader,
            catalog,
            screen: if settings.mode() == Mode::Page { Screen::Page } else { Screen::Reading },
            settings,
            battery: None,
            device: DeviceInfo::default(),
            pages: Vec::new(),
            pages_done: false,
            library: Vec::new(),
            positions: Vec::new(),
            bookmarks: Vec::new(),
            toast: None,
            serial: 0,
            flash: None,
            pressed: None,
            resume_after_menu: false,
            effects: Vec::new(),
            last_tick: 0,
        };
        app.apply_mode();
        app
    }

    pub fn toast(&self) -> Option<Toast> {
        self.toast
    }

    /// The on-screen button to draw as pressed (finger down or key just pressed).
    pub fn lit(&self, now: u64) -> Option<Hit> {
        self.pressed.or(self.flash.filter(|(_, until)| now < *until).map(|(k, _)| Hit::Key(k)))
    }

    /// Takes the pending platform requests.
    pub fn take_effect(&mut self) -> Option<Effect> {
        (!self.effects.is_empty()).then(|| self.effects.remove(0))
    }

    fn effect(&mut self, e: Effect) {
        if !self.effects.contains(&e) {
            self.effects.push(e);
        }
    }

    /// The view to return to when leaving the menu.
    fn home(&self) -> Screen {
        if self.settings.mode() == Mode::Page { Screen::Page } else { Screen::Reading }
    }

    pub fn handle(&mut self, key: Key, now: u64) -> Feedback {
        self.flash = Some((key, now + FLASH_MS));
        let fb = self.perform(action_for(self.screen, key), now);
        self.feedback(fb, now)
    }

    /// Auto-repeat while a key is held (call every ~110 ms after ~400 ms).
    /// Holding ◀◀/▶▶ while reading opens the line view and steps word by
    /// word; holding −/+ repeats the speed change; other keys just repeat.
    pub fn handle_held(&mut self, key: Key, now: u64) -> Feedback {
        if self.screen == Screen::Reading && matches!(key, Key::Left | Key::Right) {
            self.screen = Screen::Line { resume: self.reader.is_playing() };
            self.reader.pause();
        }
        self.handle(key, now)
    }

    /// The held key or finger was let go.
    pub fn release(&mut self, now: u64) {
        self.pressed = None;
        if let Screen::Line { resume } = self.screen {
            self.screen = Screen::Reading;
            if resume {
                self.reader.play(now);
            }
        }
    }

    /// A finger came down on a target: light it, act on release.
    pub fn touch_down(&mut self, hit: Option<Hit>) {
        self.pressed = hit;
    }

    /// Cancels a press (the finger slid off the button).
    pub fn touch_cancel(&mut self) {
        self.pressed = None;
    }

    /// The finger lifted on the same target it came down on.
    pub fn tap(&mut self, hit: Hit, now: u64) -> Feedback {
        self.pressed = None;
        match (hit, self.screen) {
            (Hit::Key(k), _) => self.handle(k, now),
            (Hit::Row(i), Screen::Menu { .. }) if i < MENU_LEN => {
                self.screen = Screen::Menu { sel: i };
                self.handle(Key::Select, now)
            }
            (Hit::Row(i), Screen::Pick { kind, original, .. }) if i < self.pick_len(kind) => {
                self.screen = Screen::Pick { kind, sel: i, original };
                self.handle(Key::Select, now)
            }
            (Hit::Scroll { down, rows }, Screen::Menu { .. } | Screen::Pick { .. }) => {
                let mut fb = Feedback::limit(if down { Msg::Bottom } else { Msg::Top });
                for _ in 0..rows.max(1) {
                    let step = self.perform(if down { Action::Next } else { Action::Prev }, now);
                    if step.tone == Tone::Limit {
                        break;
                    }
                    fb = step;
                }
                self.feedback(fb, now)
            }
            (Hit::Bookmark, Screen::Reading | Screen::Page) => {
                let fb = self.toggle_bookmark();
                self.feedback(fb, now)
            }
            (Hit::Highlight, Screen::Page) => {
                self.settings.highlight = !self.settings.highlight;
                self.flash = None;
                self.effect(Effect::SaveSettings);
                let fb = Feedback::ok(Msg::Highlight(self.settings.highlight));
                self.feedback(fb, now)
            }
            // A stale target from a screen that just changed: ignore quietly.
            _ => Feedback::ok(Msg::Shown),
        }
    }

    fn feedback(&mut self, fb: Feedback, now: u64) -> Feedback {
        if fb.is_toast() {
            self.serial = self.serial.wrapping_add(1);
            self.toast = Some(Toast { feedback: fb, until: now + TOAST_MS, serial: self.serial });
        }
        fb
    }

    /// Advances playback and expires toasts/flashes. True if anything visible changed.
    pub fn tick(&mut self, now: u64) -> bool {
        self.last_tick = now;
        let was_playing = self.reader.is_playing();
        let mut changed = self.reader.tick(now);
        if was_playing && !self.reader.is_playing() {
            self.serial = self.serial.wrapping_add(1);
            self.toast = Some(Toast { feedback: Feedback::ok(Msg::Finished), until: now + TOAST_MS * 2, serial: self.serial });
            self.save();
            changed = true;
        }
        if self.toast.is_some_and(|t| now >= t.until) {
            self.toast = None;
            changed = true;
        }
        if self.flash.is_some_and(|(_, until)| now >= until) {
            self.flash = None;
            changed = true;
        }
        changed
    }

    /// When the platform needs to call `tick` next (sleep until then).
    pub fn next_wake(&self) -> Option<u64> {
        // The ticker glides between words: ask for ~30 frames a second.
        let frame = (self.animating()).then_some(self.last_tick + 33);
        [self.reader.next_due(), self.toast.map(|t| t.until), self.flash.map(|f| f.1), frame].into_iter().flatten().min()
    }

    /// True while something moves between words (the ticker), so the screen
    /// must be redrawn every frame rather than once per word.
    pub fn animating(&self) -> bool {
        self.screen == Screen::Reading && self.settings.mode() == Mode::Ticker && self.reader.is_playing()
    }

    fn save(&mut self) {
        self.settings.wpm = self.reader.wpm();
        self.settings.pos = self.reader.pos() as u32;
        self.effect(Effect::SaveSettings);
    }

    /// Switches to another book (the platform loaded it after `OpenBook`).
    /// Remembers where the old one was left and resumes the new one there.
    pub fn open_book(&mut self, book: Book, index: u8) {
        let n = self.library.len().max(index as usize + 1).max(self.settings.book as usize + 1);
        self.positions.resize(n, 0);
        if !self.reader.book.is_empty() {
            self.positions[self.settings.book as usize] = self.reader.pos() as u32;
        }
        let pos = self.positions[index as usize] as usize;
        self.reader.set_book(book, pos);
        self.settings.book = index;
        self.settings.pos = self.reader.pos() as u32;
        self.pages.clear();
        self.screen = self.home();
        self.effect(Effect::SaveSettings);
    }

    /// Phrase and Page modes group words the original e-reader's way.
    pub fn groups_by_phrase(&self) -> bool {
        matches!(self.settings.mode(), Mode::Phrase | Mode::Page)
    }

    /// Groups words the way the current mode reads them.
    pub fn apply_mode(&mut self) {
        match self.settings.mode() {
            Mode::Word => {
                self.reader.set_phrase(None);
                self.reader.set_chunk(self.settings.chunk);
            }
            Mode::Phrase | Mode::Page => self.reader.set_phrase(Some(self.settings.phrase)),
            Mode::Ticker => {
                self.reader.set_phrase(None);
                self.reader.set_chunk(1);
            }
        }
    }

    /// The open book's bookmarks, in reading order.
    pub fn book_marks(&self) -> Vec<u32> {
        let book = self.settings.book;
        let mut v: Vec<u32> = self.bookmarks.iter().filter(|(b, _)| *b == book).map(|(_, w)| *w).collect();
        v.sort_unstable();
        v
    }

    /// Whether any bookmark of the open book falls in `r` (words).
    pub fn bookmarked(&self, r: core::ops::Range<usize>) -> bool {
        let book = self.settings.book;
        self.bookmarks.iter().any(|&(b, w)| b == book && r.contains(&(w as usize)))
    }

    /// Dog-ears the place being read, or removes the bookmark that is here.
    fn toggle_bookmark(&mut self) -> Feedback {
        let book = self.settings.book;
        let here = self.reader.shown();
        let before = self.bookmarks.len();
        self.bookmarks.retain(|&(b, w)| !(b == book && here.contains(&(w as usize))));
        self.effect(Effect::SaveBookmarks);
        if self.bookmarks.len() < before {
            return Feedback::ok(Msg::BookmarkRemoved);
        }
        self.bookmarks.push((book, self.reader.pos() as u32));
        Feedback::ok(Msg::Bookmarked)
    }

    /// "37%  ·  it was a small device…" for the Bookmarks list.
    fn bookmark_label(&self, w: usize) -> String {
        let b = &self.reader.book;
        let pct = if b.len() > 1 { w * 100 / (b.len() - 1) } else { 0 };
        let mut s = format!("{pct}%  ");
        for i in w..(w + 6).min(b.len()) {
            s.push(' ');
            s.push_str(b.text_of(b.word(i).unwrap()));
        }
        s.push('…');
        s
    }

    /// A book arrived from the browser and is now in the library.
    pub fn book_added(&mut self, title: String, now: u64) {
        self.library.push(title);
        self.positions.resize(self.library.len(), 0);
        let fb = Feedback::ok(Msg::BookAdded);
        self.feedback(fb, now);
    }

    /// Uploaded books were removed: the library is back to its first `keep`.
    pub fn books_removed(&mut self, keep: usize, now: u64) {
        self.library.truncate(keep);
        self.positions.truncate(keep);
        self.bookmarks.retain(|&(b, _)| (b as usize) < keep);
        self.effect(Effect::SaveBookmarks);
        if self.settings.book as usize >= keep {
            // The open book was one of them: close it and open the first book.
            self.reader.set_book(Book::new("", ""), 0);
            self.settings.book = 0;
            self.pages.clear();
            self.effect(Effect::OpenBook(0));
        }
        let fb = Feedback::ok(Msg::BooksRemoved);
        self.feedback(fb, now);
    }

    /// Frees the open book before a big one is loaded (memory is tight on
    /// the device), remembering where it was left.
    pub fn park_book(&mut self) {
        let n = self.library.len().max(self.settings.book as usize + 1);
        self.positions.resize(n, 0);
        if !self.reader.book.is_empty() {
            self.positions[self.settings.book as usize] = self.reader.pos() as u32;
        }
        self.reader.set_book(Book::new("", ""), 0);
    }

    /// Changes one setting by name (the web settings page). Validated and
    /// clamped in [`Settings::set`]; applied to the reader and the hardware.
    pub fn set_setting(&mut self, name: &str, value: u16) -> Result<(), String> {
        let mut s = self.settings;
        s.set(name, value)?;
        s.theme = s.theme.min(self.catalog.themes.len().saturating_sub(1) as u8);
        s.font = s.font.min(self.catalog.fonts.len().saturating_sub(1) as u8);
        let display = s.brightness != self.settings.brightness || s.flipped != self.settings.flipped;
        if s.wifi != self.settings.wifi {
            self.effect(Effect::Wifi(s.wifi));
        }
        let view_changed = s.mode != self.settings.mode;
        self.settings = s;
        self.reader.set_wpm(s.wpm);
        self.reader.set_pacing(s.pacing);
        self.reader.set_true_pace(s.true_pace);
        self.apply_mode();
        if display {
            self.effect(Effect::Display);
        }
        if view_changed && !self.screen.is_overlay() {
            self.reader.pause();
            self.screen = self.home();
        }
        self.effect(Effect::SaveSettings);
        Ok(())
    }

    // ------------------------------------------------------------ pages

    /// Index of the page containing the current word.
    /// `None` while the reader is beyond the pages measured so far.
    pub fn page_index(&self) -> Option<usize> {
        if self.pages.is_empty() {
            return None;
        }
        let pos = self.reader.pos() as u32;
        let i = self.pages.partition_point(|&s| s <= pos);
        // In the last measured page we don't know where it ends yet.
        if i == self.pages.len() && !self.pages_done {
            return None;
        }
        Some(i.saturating_sub(1))
    }

    // ------------------------------------------------------------ actions

    fn perform(&mut self, action: Action, now: u64) -> Feedback {
        let screen = self.screen;
        match (screen, action) {
            (_, Action::Faster | Action::Slower) => {
                let changed = self.reader.nudge_wpm(action == Action::Faster);
                self.settings.wpm = self.reader.wpm();
                if changed {
                    self.effect(Effect::SaveSettings);
                }
                let msg = Msg::Speed(self.reader.wpm());
                if changed { Feedback::ok(msg) } else { Feedback::limit(msg) }
            }
            (_, Action::SkipBack | Action::SkipForward) => {
                let forward = action == Action::SkipForward;
                let skip = self.settings.skip();
                let moved = if forward { self.reader.skip_forward(skip, now) } else { self.reader.skip_back(skip, now) };
                if moved {
                    Feedback::ok(Msg::Skipped { forward, skip })
                } else {
                    Feedback::limit(if forward { Msg::AtEnd } else { Msg::AtStart })
                }
            }
            (_, Action::WordBack | Action::WordForward) => {
                let fwd = action == Action::WordForward;
                if self.reader.step_word(fwd, now) {
                    Feedback::ok(Msg::Shown)
                } else {
                    Feedback::limit(if fwd { Msg::AtEnd } else { Msg::AtStart })
                }
            }
            (_, Action::PhraseBack | Action::PhraseForward) => {
                let forward = action == Action::PhraseForward;
                if self.reader.step_phrase(forward, now) {
                    self.settings.pos = self.reader.pos() as u32;
                    // The highlight moving (and the page turning) is the feedback.
                    Feedback::ok(Msg::Shown)
                } else {
                    Feedback::limit(if forward { Msg::AtEnd } else { Msg::AtStart })
                }
            }
            (Screen::Line { .. }, Action::TogglePlay) => {
                self.screen = Screen::Reading;
                self.reader.play(now);
                Feedback::ok(Msg::Playing)
            }
            (_, Action::TogglePlay) => {
                if self.reader.is_playing() {
                    self.reader.pause();
                    self.save();
                    Feedback::ok(Msg::Paused)
                } else {
                    self.reader.play(now);
                    Feedback::ok(Msg::Playing)
                }
            }
            (_, Action::OpenMenu) => {
                self.resume_after_menu = self.reader.is_playing() || matches!(screen, Screen::Line { resume: true });
                self.reader.pause();
                self.screen = Screen::Menu { sel: 0 };
                Feedback::ok(Msg::Shown)
            }
            (_, Action::CloseMenu) => {
                if let Screen::Pick { kind, original, .. } = screen {
                    if kind.live_preview() {
                        self.preview(kind, original);
                    }
                }
                self.close_menu(now);
                Feedback::ok(Msg::Shown)
            }
            (Screen::Menu { sel }, Action::Prev | Action::Next) => match step(sel, MENU_LEN, action == Action::Next) {
                Some(s) => {
                    self.screen = Screen::Menu { sel: s };
                    Feedback::ok(Msg::Shown)
                }
                None => Feedback::limit(if action == Action::Next { Msg::Bottom } else { Msg::Top }),
            },
            (Screen::Pick { kind, sel, original }, Action::Prev | Action::Next) => {
                match step(sel, self.pick_len(kind), action == Action::Next) {
                    Some(s) => {
                        if kind.live_preview() {
                            self.preview(kind, s);
                        }
                        self.screen = Screen::Pick { kind, sel: s, original };
                        Feedback::ok(Msg::Shown)
                    }
                    None => Feedback::limit(if action == Action::Next { Msg::Bottom } else { Msg::Top }),
                }
            }
            (Screen::Info, Action::Prev) => Feedback::limit(Msg::Top),
            (Screen::Info, Action::Next) => Feedback::limit(Msg::Bottom),
            (Screen::Menu { sel }, Action::Activate) => self.activate(menu_item(sel), now),
            (Screen::Pick { kind, sel, .. }, Action::Activate) => self.choose(kind, sel, now),
            (Screen::Menu { .. }, Action::Back) => {
                self.close_menu(now);
                Feedback::ok(Msg::Shown)
            }
            (Screen::Pick { kind, original, .. }, Action::Back) => {
                if kind.live_preview() {
                    self.preview(kind, original);
                }
                self.screen = Screen::Menu { sel: menu_index(item_for(kind)) };
                Feedback::ok(Msg::Kept)
            }
            (Screen::Info, Action::Back) => {
                self.screen = Screen::Menu { sel: menu_index(MenuItem::Info) };
                Feedback::ok(Msg::Shown)
            }
            // Not reachable through the tables (checked by a test).
            _ => Feedback::limit(Msg::Shown),
        }
    }

    fn activate(&mut self, item: MenuItem, now: u64) -> Feedback {
        if let Some(kind) = item.pick() {
            if kind == PickKind::Chapter && self.reader.book.chapters().is_empty() {
                return Feedback::limit(Msg::NoChapters);
            }
            if kind == PickKind::Bookmark && self.book_marks().is_empty() {
                return Feedback::limit(Msg::NoBookmarks);
            }
            let cur = self.current_pick(kind);
            self.screen = Screen::Pick { kind, sel: cur, original: cur };
            return Feedback::ok(Msg::Shown);
        }
        match item {
            MenuItem::Resume => {
                self.resume_after_menu = true;
                self.close_menu(now);
                Feedback::ok(Msg::Playing)
            }
            MenuItem::Restart => {
                self.reader.seek(0, now);
                self.resume_after_menu = false;
                self.close_menu(now);
                self.save();
                Feedback::ok(Msg::Restarted)
            }
            MenuItem::Wifi => {
                self.settings.wifi = !self.settings.wifi;
                self.effect(Effect::Wifi(self.settings.wifi));
                self.save();
                Feedback::ok(Msg::Wifi(self.settings.wifi))
            }
            MenuItem::Rotate => {
                self.settings.flipped = !self.settings.flipped;
                self.effect(Effect::Display);
                self.save();
                Feedback::ok(Msg::Rotated)
            }
            MenuItem::Info => {
                self.screen = Screen::Info;
                Feedback::ok(Msg::Shown)
            }
            MenuItem::Sleep => {
                self.save();
                self.effect(Effect::Sleep);
                Feedback::ok(Msg::Sleeping)
            }
            _ => Feedback::limit(Msg::Shown),
        }
    }

    fn choose(&mut self, kind: PickKind, sel: u8, now: u64) -> Feedback {
        if kind == PickKind::Book {
            self.resume_after_menu = false;
            if sel != self.settings.book && (sel as usize) < self.library.len() {
                self.effect(Effect::OpenBook(sel));
            }
            self.close_menu(now);
            return Feedback::ok(Msg::Set(kind, sel));
        }
        if kind == PickKind::Mode {
            // Switching mode ends the errand: straight into the new view.
            self.settings.mode = sel;
            self.apply_mode();
            self.save();
            self.resume_after_menu = false;
            self.close_menu(now);
            return Feedback::ok(Msg::Set(kind, sel));
        }
        match kind {
            PickKind::Speed => {
                self.reader.set_wpm(SPEED_PRESETS[sel as usize]);
                self.settings.wpm = self.reader.wpm();
            }
            PickKind::Words => {
                // One "Words per flash" item: it sets the active mode's group size.
                if self.groups_by_phrase() {
                    self.settings.phrase = sel + 1;
                } else {
                    self.settings.chunk = sel + 1;
                }
                self.apply_mode();
            }
            PickKind::Skip => self.settings.skip = sel,
            PickKind::Pace => {
                self.settings.true_pace = sel == 1;
                self.reader.set_true_pace(sel == 1);
            }
            PickKind::Chapter => {
                let start = self.reader.book.chapters()[sel as usize].0 as usize;
                self.reader.seek(start, now);
            }
            PickKind::Bookmark => {
                if let Some(&w) = self.book_marks().get(sel as usize) {
                    self.reader.seek(w as usize, now);
                }
            }
            PickKind::GoTo => {
                let n = self.reader.book.len();
                self.reader.seek(n * sel as usize / 10, now);
            }
            _ => self.preview(kind, sel),
        }
        self.save();
        if matches!(kind, PickKind::Chapter | PickKind::GoTo | PickKind::Bookmark) {
            // Jumping is the end of the errand: back to reading, paused.
            self.resume_after_menu = false;
            self.close_menu(now);
        } else {
            self.screen = Screen::Menu { sel: menu_index(item_for(kind)) };
        }
        Feedback::ok(Msg::Set(kind, sel))
    }

    fn close_menu(&mut self, now: u64) {
        self.screen = self.home();
        if core::mem::take(&mut self.resume_after_menu) && self.screen == Screen::Reading {
            self.reader.play(now);
        }
    }

    pub fn pick_len(&self, kind: PickKind) -> u8 {
        match kind {
            PickKind::Book => self.library.len().clamp(1, 250) as u8,
            PickKind::Bookmark => self.book_marks().len().min(250) as u8,
            PickKind::Parens => 2,
            PickKind::Pace => 2,
            PickKind::Mode => MODE_NAMES.len() as u8,
            PickKind::Spacing => SPACING_NAMES.len() as u8,
            PickKind::Paragraphs => PARAGRAPH_NAMES.len() as u8,
            PickKind::Align => ALIGN_NAMES.len() as u8,
            PickKind::Speed => SPEED_PRESETS.len() as u8,
            PickKind::Words | PickKind::Size => 3,
            PickKind::Skip => SKIPS.len() as u8,
            PickKind::Chapter => self.reader.book.chapters().len().min(250) as u8,
            PickKind::GoTo => 10,
            PickKind::Font => self.catalog.fonts.len() as u8,
            PickKind::Theme => self.catalog.themes.len() as u8,
            PickKind::Brightness => 5,
        }
    }

    pub fn pick_label(&self, kind: PickKind, i: u8) -> String {
        let i = i as usize;
        match kind {
            PickKind::Book => self.library.get(i).cloned().unwrap_or_else(|| self.reader.book.title.clone()),
            PickKind::Bookmark => self.bookmark_label(self.book_marks().get(i).copied().unwrap_or(0) as usize),
            PickKind::Parens => PAREN_NAMES[i].into(),
            PickKind::Pace => PACE_NAMES[i].into(),
            PickKind::Mode => MODE_NAMES[i].into(),
            PickKind::Spacing => SPACING_NAMES[i].into(),
            PickKind::Paragraphs => PARAGRAPH_NAMES[i].into(),
            PickKind::Align => ALIGN_NAMES[i].into(),
            PickKind::Speed => format!("{} wpm", SPEED_PRESETS[i]),
            PickKind::Words => CHUNK_NAMES[i].into(),
            PickKind::Skip => SKIP_NAMES[i].into(),
            PickKind::Chapter => self.reader.book.chapters()[i].1.clone(),
            PickKind::GoTo => format!("{}%", i * 10),
            PickKind::Size => SIZE_NAMES[i].into(),
            PickKind::Font => self.catalog.fonts[i].into(),
            PickKind::Theme => self.catalog.themes[i].into(),
            PickKind::Brightness => BRIGHTNESS_NAMES[i].into(),
        }
    }

    /// The current value shown next to a menu item.
    pub fn menu_value(&self, item: MenuItem) -> Option<String> {
        Some(match item {
            MenuItem::Speed => format!("{} wpm", self.reader.wpm()),
            MenuItem::Words => format!("{}", if self.groups_by_phrase() { self.settings.phrase } else { self.settings.chunk }),
            MenuItem::Chapters => format!("{}", self.reader.book.chapters().len()),
            MenuItem::Bookmarks => format!("{}", self.book_marks().len()),
            MenuItem::GoTo => format!("{}%", self.reader.permille() / 10),
            MenuItem::Rotate => (if self.settings.flipped { "180°" } else { "0°" }).into(),
            MenuItem::Wifi => (if self.settings.wifi { "On" } else { "Off" }).into(),
            MenuItem::Info | MenuItem::Resume | MenuItem::Restart | MenuItem::Sleep => return None,
            other => {
                let kind = other.pick()?;
                self.pick_label(kind, self.current_pick(kind))
            }
        })
    }

    /// The choice that is currently applied (marked with a dot in lists).
    pub fn current_pick(&self, kind: PickKind) -> u8 {
        match kind {
            PickKind::Book => self.settings.book.min(self.pick_len(kind) - 1),
            PickKind::Bookmark => {
                let pos = self.reader.pos() as u32;
                self.book_marks().iter().rposition(|&b| b <= pos).unwrap_or(0) as u8
            }
            PickKind::Parens => self.settings.parens as u8,
            PickKind::Pace => self.settings.true_pace as u8,
            PickKind::Mode => self.settings.mode,
            PickKind::Spacing => self.settings.spacing,
            PickKind::Paragraphs => self.settings.indent as u8,
            PickKind::Align => self.settings.justify as u8,
            PickKind::Speed => {
                let w = self.reader.wpm();
                SPEED_PRESETS.iter().enumerate().min_by_key(|(_, p)| p.abs_diff(w)).map_or(0, |(i, _)| i as u8)
            }
            PickKind::Words => (if self.groups_by_phrase() { self.settings.phrase } else { self.settings.chunk }).clamp(1, 3) - 1,
            PickKind::Skip => self.settings.skip,
            PickKind::Chapter => self.reader.book.chapter_index(self.reader.pos()).unwrap_or(0) as u8,
            PickKind::GoTo => (self.reader.permille() / 100).min(9) as u8,
            PickKind::Size => self.settings.size,
            PickKind::Font => self.settings.font,
            PickKind::Theme => self.settings.theme,
            PickKind::Brightness => self.settings.brightness - 1,
        }
    }

    /// Applies a look setting immediately (live preview, or the final choice).
    fn preview(&mut self, kind: PickKind, v: u8) {
        match kind {
            PickKind::Size => self.settings.size = v,
            PickKind::Font => self.settings.font = v,
            PickKind::Theme => self.settings.theme = v,
            PickKind::Parens => self.settings.parens = v == 1,
            PickKind::Spacing => self.settings.spacing = v,
            PickKind::Paragraphs => self.settings.indent = v == 1,
            PickKind::Align => self.settings.justify = v == 1,
            PickKind::Brightness => {
                self.settings.brightness = v + 1;
                self.effect(Effect::Display);
            }
            _ => {}
        }
    }
}

fn item_for(kind: PickKind) -> MenuItem {
    menu_items().find(|i| i.pick() == Some(kind)).unwrap_or(MenuItem::Resume)
}

/// Moves a list selection; `None` at either end (lists don't wrap, like speed).
fn step(sel: u8, len: u8, down: bool) -> Option<u8> {
    if down {
        (sel + 1 < len).then_some(sel + 1)
    } else {
        sel.checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Key::*;

    const CAT: Catalog = Catalog { themes: &["A", "B", "C"], fonts: &["x", "y"] };
    const TEXT: &str = "# One\nAlpha beta. Gamma delta.\n# Two\nEpsilon zeta eta. Theta.";

    fn app() -> App {
        App::new(Book::new("t", TEXT), CAT, Settings::default())
    }

    /// Everything the user could see, to check that each key does *something*.
    fn visible(a: &App) -> (Screen, usize, u16, bool, Settings) {
        (a.screen, a.reader.pos(), a.reader.wpm(), a.reader.is_playing(), a.settings)
    }

    fn all_screens() -> Vec<Screen> {
        let mut v = alloc::vec![Screen::Reading, Screen::Line { resume: false }, Screen::Page, Screen::Info];
        for i in 0..MENU_LEN {
            v.push(Screen::Menu { sel: i });
        }
        for kind in [PickKind::Book, PickKind::Parens, PickKind::Pace, PickKind::Speed, PickKind::Words, PickKind::Skip, PickKind::Chapter, PickKind::GoTo, PickKind::Size, PickKind::Font, PickKind::Theme, PickKind::Brightness] {
            v.push(Screen::Pick { kind, sel: 0, original: 0 });
            v.push(Screen::Pick { kind, sel: 1, original: 0 });
        }
        v
    }

    #[test]
    fn menu_len_matches_groups() {
        assert_eq!(menu_items().count(), MENU_LEN as usize);
    }

    #[test]
    fn every_key_on_every_screen_gives_feedback() {
        for screen in all_screens() {
            for pos in [0, 4, 10] {
                for key in Key::ALL {
                    let mut a = app();
                    a.reader.seek(pos, 0);
                    a.screen = screen;
                    let before = visible(&a);
                    let fb = a.handle(key, 0);
                    let changed = visible(&a) != before;
                    let effect = a.take_effect().is_some();
                    assert!(changed || fb.is_toast() || effect, "{key:?} on {screen:?} at {pos}: nothing visible");
                    if !changed && !effect {
                        assert_eq!(fb.tone, Tone::Limit, "{key:?} on {screen:?} at {pos}: no change must say why");
                    }
                    assert_ne!(fb, Feedback::limit(Msg::Shown), "{key:?} on {screen:?} reached an unbound action");
                }
            }
        }
    }

    #[test]
    fn same_key_same_meaning_everywhere() {
        // Left goes back one level, Menu always leaves the menu system.
        let mut a = app();
        a.handle(Menu, 0);
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Font) };
        a.handle(Right, 0);
        assert!(matches!(a.screen, Screen::Pick { kind: PickKind::Font, .. }));
        a.handle(Left, 0);
        assert_eq!(a.screen, Screen::Menu { sel: menu_index(MenuItem::Font) });
        a.handle(Left, 0);
        assert_eq!(a.screen, Screen::Reading);
        for s in all_screens() {
            let mut a = app();
            a.screen = s;
            a.handle(Menu, 0);
            let expect = if s.is_overlay() { Screen::Reading } else { Screen::Menu { sel: 0 } };
            assert_eq!(a.screen, expect, "Menu on {s:?}");
        }
    }

    #[test]
    fn taps_equal_keys() {
        let mut a = app();
        let mut b = app();
        a.tap(Hit::Key(Up), 0);
        b.handle(Up, 0);
        assert_eq!(visible(&a), visible(&b));
        a.tap(Hit::Key(Menu), 0);
        a.tap(Hit::Row(menu_index(MenuItem::Theme)), 0);
        assert!(matches!(a.screen, Screen::Pick { kind: PickKind::Theme, .. }));
        a.tap(Hit::Row(2), 0);
        assert_eq!(a.settings.theme, 2);
        assert_eq!(a.screen, Screen::Menu { sel: menu_index(MenuItem::Theme) });
    }

    #[test]
    fn pressed_button_lights_until_release() {
        let mut a = app();
        a.touch_down(Some(Hit::Key(Select)));
        assert_eq!(a.lit(0), Some(Hit::Key(Select)));
        a.touch_cancel();
        assert_eq!(a.lit(0), None, "sliding off cancels without acting");
        assert!(!a.reader.is_playing());
        a.handle(Up, 100);
        assert_eq!(a.lit(150), Some(Hit::Key(Up)), "physical keys light their on-screen twin");
        assert_eq!(a.lit(100 + FLASH_MS), None);
    }

    #[test]
    fn scroll_buttons_page_through_lists() {
        let mut a = app();
        a.handle(Menu, 0);
        a.tap(Hit::Scroll { down: true, rows: 5 }, 0);
        assert_eq!(a.screen, Screen::Menu { sel: 5 });
        a.tap(Hit::Scroll { down: true, rows: 50 }, 0);
        assert_eq!(a.screen, Screen::Menu { sel: MENU_LEN - 1 });
        assert_eq!(a.tap(Hit::Scroll { down: true, rows: 5 }, 0).tone, Tone::Limit);
    }

    #[test]
    fn theme_preview_and_cancel_restores() {
        let mut a = app();
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Theme) };
        a.handle(Select, 0);
        a.handle(Down, 0);
        assert_eq!(a.settings.theme, 1, "scrolling previews live");
        assert_eq!(a.handle(Left, 0).msg, Msg::Kept);
        assert_eq!(a.settings.theme, 0, "back restores");
        a.handle(Select, 0);
        a.handle(Down, 0);
        a.handle(Down, 0);
        assert_eq!(a.handle(Select, 0).msg, Msg::Set(PickKind::Theme, 2));
        assert_eq!(a.settings.theme, 2);
    }

    #[test]
    fn speed_list_does_not_preview_but_applies_on_choose() {
        let mut a = app();
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Speed) };
        a.handle(Select, 0);
        a.handle(Down, 0);
        assert_eq!(a.reader.wpm(), 475, "browsing the list changes nothing");
        a.handle(Select, 0);
        assert_eq!(a.reader.wpm(), 500);
    }

    #[test]
    fn chapters_jump_and_return_to_reading() {
        let mut a = app();
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Chapters) };
        a.handle(Select, 0);
        a.handle(Down, 0);
        a.handle(Select, 0);
        assert_eq!(a.screen, Screen::Reading);
        assert_eq!(a.reader.pos(), 5, "first word of '# Two'");
        assert!(!a.reader.is_playing());
    }

    #[test]
    fn no_chapters_is_explained() {
        let mut a = App::new(Book::new("t", "plain text only"), CAT, Settings::default());
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Chapters) };
        assert_eq!(a.handle(Select, 0), Feedback::limit(Msg::NoChapters));
    }

    fn choose_mode(a: &mut App, m: Mode) {
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Mode) };
        a.handle(Select, 0);
        let target = crate::settings::MODES.iter().position(|x| *x == m).unwrap() as u8;
        if let Screen::Pick { kind, original, .. } = a.screen {
            a.screen = Screen::Pick { kind, sel: target, original };
        }
        a.handle(Select, 0);
    }

    #[test]
    fn words_per_flash_sets_the_active_modes_group_size() {
        let mut a = app();
        choose_mode(&mut a, Mode::Phrase);
        assert_eq!(a.menu_value(MenuItem::Words).unwrap(), "3", "phrases default to 3, as on the original");
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Words) };
        a.handle(Select, 0);
        a.handle(Up, 0);
        a.handle(Select, 0);
        assert_eq!((a.settings.phrase, a.settings.chunk), (2, 1), "word mode keeps its own value");
        choose_mode(&mut a, Mode::Word);
        assert_eq!(a.menu_value(MenuItem::Words).unwrap(), "1");
    }

    #[test]
    fn wifi_toggle_asks_the_platform() {
        let mut a = app();
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Wifi) };
        assert_eq!(a.handle(Select, 0).msg, Msg::Wifi(true));
        assert!(core::iter::from_fn(|| a.take_effect()).any(|e| e == Effect::Wifi(true)));
        assert_eq!(a.menu_value(MenuItem::Wifi).unwrap(), "On");
    }

    #[test]
    fn page_mode_steps_phrase_by_phrase() {
        let mut a = App::new(Book::new("t", "She walked into the old house of the family, and waited. Then left."), CAT, Settings::default());
        choose_mode(&mut a, Mode::Page);
        assert_eq!(a.screen, Screen::Page);
        assert!(a.reader.phrase_mode(), "page mode groups by phrases");
        assert_eq!(a.handle(Right, 0).msg, Msg::Shown, "the highlight moving is the feedback");
        assert_eq!(a.reader.pos(), 3);
        a.handle(Right, 0);
        assert_eq!(a.reader.pos(), 6);
        a.handle(Left, 0);
        assert_eq!(a.reader.pos(), 3);
        a.handle_held(Right, 500);
        a.handle_held(Right, 610);
        assert_eq!(a.reader.pos(), 9, "holding repeats");
        assert_eq!(a.screen, Screen::Page, "holding doesn't open the line view in page mode");
        a.reader.seek(11, 0);
        assert_eq!(a.handle(Right, 0).tone, Tone::Limit, "end of book");
        a.handle(Up, 0);
        assert_eq!(a.reader.wpm(), 500, "▲ is speed in every mode");
        a.handle(Menu, 0);
        a.handle(Menu, 0);
        assert_eq!(a.screen, Screen::Page, "menu returns to the view you were in");
    }

    #[test]
    fn highlight_dot_toggles_in_page_mode_only() {
        let mut a = app();
        assert!(a.settings.highlight);
        a.tap(Hit::Highlight, 0);
        assert!(a.settings.highlight, "ignored outside page mode");
        choose_mode(&mut a, Mode::Page);
        assert_eq!(a.tap(Hit::Highlight, 0).msg, Msg::Highlight(false));
        assert!(!a.settings.highlight);
    }

    #[test]
    fn each_mode_groups_words_its_own_way() {
        let mut a = App::new(Book::new("t", "She walked into the old house of the family."), CAT, Settings { chunk: 2, ..Settings::default() });
        assert_eq!(a.reader.shown(), 0..2, "word mode: 2 words per flash");
        choose_mode(&mut a, Mode::Phrase);
        assert_eq!(a.screen, Screen::Reading);
        a.reader.seek(2, 0);
        assert_eq!(a.reader.shown(), 2..5, "phrases: 3 words, 'into the old'");
        choose_mode(&mut a, Mode::Ticker);
        assert_eq!(a.reader.shown(), 2..3, "ticker: one word at a time");
        a.handle(Select, 0);
        a.tick(100);
        assert!(a.animating());
        assert!(a.next_wake().unwrap() <= 133, "about 30 frames a second while gliding");
    }

    #[test]
    fn rotate_and_brightness_ask_the_platform() {
        let mut a = app();
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Rotate) };
        a.handle(Select, 0);
        assert!(a.settings.flipped);
        assert_eq!(a.take_effect(), Some(Effect::Display));
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Brightness) };
        a.handle(Select, 0);
        a.handle(Up, 0);
        assert_eq!(a.settings.brightness, 3, "live preview");
        assert!(core::iter::from_fn(|| a.take_effect()).any(|e| e == Effect::Display));
    }

    #[test]
    fn menu_pauses_and_restores_playback() {
        let mut a = app();
        a.handle(Select, 0);
        a.handle(Menu, 10);
        assert!(!a.reader.is_playing());
        a.handle(Left, 20);
        assert!(a.reader.is_playing());
    }

    #[test]
    fn holding_back_scrolls_then_release_resumes() {
        let mut a = app();
        a.reader.seek(4, 0);
        a.handle(Select, 0);
        a.handle_held(Left, 500);
        assert_eq!(a.screen, Screen::Line { resume: true });
        a.handle_held(Left, 620);
        assert_eq!(a.reader.pos(), 2);
        a.release(700);
        assert_eq!(a.screen, Screen::Reading);
        assert!(a.reader.is_playing());
        assert_eq!(a.reader.pos(), 2);
    }

    #[test]
    fn holding_plus_repeats_speed() {
        let mut a = app();
        a.handle_held(Up, 0);
        a.handle_held(Up, 120);
        assert_eq!(a.screen, Screen::Reading);
        assert_eq!(a.reader.wpm(), 525, "475 + 2 × 25");
    }

    #[test]
    fn toast_expires_and_idle_needs_no_wakeups() {
        let mut a = app();
        a.handle(Up, 0);
        assert!(a.tick(TOAST_MS));
        assert!(a.toast().is_none());
        assert_eq!(a.next_wake(), None);
    }

    #[test]
    fn finishing_saves_and_says_so() {
        let mut a = App::new(Book::new("t", "a b"), CAT, Settings::default());
        a.handle(Select, 0);
        let mut now = 0;
        while a.reader.is_playing() {
            now += 50;
            a.tick(now);
        }
        assert_eq!(a.toast().unwrap().feedback.msg, Msg::Finished);
        assert!(core::iter::from_fn(|| a.take_effect()).any(|e| e == Effect::SaveSettings));
    }

    #[test]
    fn menu_values_reflect_state() {
        let a = app();
        assert_eq!(a.menu_value(MenuItem::Speed).unwrap(), "475 wpm");
        assert_eq!(a.menu_value(MenuItem::Theme).unwrap(), "A");
        assert_eq!(a.menu_value(MenuItem::Size).unwrap(), "Normal");
        assert_eq!(a.menu_value(MenuItem::Chapters).unwrap(), "2");
    }

    #[test]
    fn skip_setting_changes_what_the_skip_buttons_do() {
        let mut a = App::new(Book::new("t", "a b c d e f g h i j k l m n o p."), CAT, Settings::default());
        a.handle(Right, 0);
        assert_eq!(a.reader.pos(), 0, "one sentence: nowhere to go");
        a.set_setting("skip", 2).unwrap();
        assert_eq!(a.handle(Right, 0).msg, Msg::Skipped { forward: true, skip: Skip::Words(10) });
        assert_eq!(a.reader.pos(), 10);
    }

    #[test]
    fn web_settings_apply_immediately_and_are_validated() {
        let mut a = app();
        a.set_setting("wpm", 450).unwrap();
        assert_eq!(a.reader.wpm(), 450);
        a.set_setting("chunk", 3).unwrap();
        assert_eq!(a.reader.chunk(), 3);
        a.set_setting("theme", 99).unwrap();
        assert_eq!(a.settings.theme, 2, "clamped to the themes that exist");
        a.set_setting("brightness", 2).unwrap();
        assert!(core::iter::from_fn(|| a.take_effect()).any(|e| e == Effect::Display));
        a.set_setting("mode", 3).unwrap();
        assert_eq!(a.screen, Screen::Page);
        assert!(a.set_setting("nonsense", 1).is_err());
        let before = a.reader.ms_left();
        a.set_setting("sentence", 50).unwrap();
        assert!(a.reader.ms_left() > before, "longer pauses, longer time left");
    }

    #[test]
    fn library_switches_books_and_remembers_positions() {
        let mut a = app();
        a.library = alloc::vec!["First".into(), "Second".into()];
        a.reader.seek(7, 0);
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Library) };
        a.handle(Select, 0);
        assert!(matches!(a.screen, Screen::Pick { kind: PickKind::Book, sel: 0, .. }));
        a.handle(Down, 0);
        a.handle(Select, 0);
        assert_eq!(a.take_effect(), Some(Effect::OpenBook(1)));
        a.open_book(Book::new("Second", "uno due tre quattro."), 1);
        assert_eq!(a.reader.book.title, "Second");
        assert_eq!(a.reader.pos(), 0);
        a.reader.seek(2, 0);
        a.open_book(Book::new("t", TEXT), 0);
        assert_eq!(a.reader.pos(), 7, "back where the first book was left");
        assert_eq!(a.positions[1], 2);
    }

    #[test]
    fn removing_the_open_uploaded_book_returns_to_the_first() {
        let mut a = app();
        a.library = alloc::vec!["Built in".into()];
        a.reader.seek(7, 0);
        a.book_added("Uploaded".into(), 0);
        a.open_book(Book::new("Uploaded", "uno due tre"), 1);
        a.books_removed(1, 10);
        assert_eq!(a.library.len(), 1);
        assert!(core::iter::from_fn(|| a.take_effect()).any(|e| e == Effect::OpenBook(0)));
        a.open_book(Book::new("t", TEXT), 0);
        assert_eq!(a.reader.pos(), 7, "the built-in book's place is kept");
    }

    #[test]
    fn bookmarks_toggle_list_and_jump() {
        let mut a = app();
        a.handle(Menu, 0);
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Bookmarks) };
        assert_eq!(a.handle(Select, 0), Feedback::limit(Msg::NoBookmarks), "explained when empty");
        a.handle(Menu, 0);
        a.reader.seek(6, 0);
        assert_eq!(a.tap(Hit::Bookmark, 0).msg, Msg::Bookmarked);
        assert!(a.bookmarked(6..7));
        assert!(core::iter::from_fn(|| a.take_effect()).any(|e| e == Effect::SaveBookmarks));
        a.reader.seek(2, 0);
        a.tap(Hit::Bookmark, 0);
        assert_eq!(a.book_marks(), [2, 6], "listed in reading order");
        a.screen = Screen::Menu { sel: menu_index(MenuItem::Bookmarks) };
        a.handle(Select, 0);
        a.handle(Down, 0);
        assert!(a.pick_label(PickKind::Bookmark, 1).contains("Epsilon"), "{}", a.pick_label(PickKind::Bookmark, 1));
        a.handle(Select, 0);
        assert_eq!(a.reader.pos(), 6, "jumped to the bookmark");
        assert_eq!(a.screen, Screen::Reading);
        assert_eq!(a.tap(Hit::Bookmark, 0).msg, Msg::BookmarkRemoved, "tapping the corner again removes it");
        assert_eq!(a.book_marks(), [2]);
    }

    #[test]
    fn bookmarks_belong_to_their_book() {
        let mut a = app();
        a.library = alloc::vec!["A".into(), "B".into()];
        a.reader.seek(3, 0);
        a.tap(Hit::Bookmark, 0);
        a.open_book(Book::new("B", "uno due tre quattro"), 1);
        assert!(a.book_marks().is_empty());
        a.open_book(Book::new("t", TEXT), 0);
        assert_eq!(a.book_marks(), [3]);
    }

    #[test]
    fn saved_view_and_chunk_are_restored() {
        let s = Settings { mode: 3, chunk: 2, theme: 200, ..Settings::default() };
        let a = App::new(Book::new("t", TEXT), CAT, s);
        assert_eq!(a.screen, Screen::Page);
        assert_eq!(a.reader.chunk(), 2);
        assert!(a.reader.phrase_mode());
        assert!(a.settings.theme < 3);
    }
}

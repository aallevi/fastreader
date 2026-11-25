//! Turning text into a word list with pacing and style hints.
//!
//! Input is plain text with a light Markdown subset, which is what the EPUB/PDF
//! converter produces:
//!
//! * `# Chapter One` (1–6 `#`) – a heading: shown bold, twice as long
//! * `**bold**`, `*italic*`, `_italic_` – kept as bold / italic
//! * a blank line – paragraph break (longer pause)
//!
//! Words are stored cleaned (markers removed) in one string, 12 bytes of
//! metadata each, so a whole novel fits easily in the ESP32's PSRAM.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::chunked::{Chunked, ChunkedText};
use crate::settings::Pacing;

const CHECKPOINT: usize = 64;

/// How long the reader lingers after a word, decided by what ends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pause {
    Normal,
    /// After a comma or a dash: a breath.
    Clause,
    /// After a semicolon or a colon: a half stop, a little longer than a
    /// comma's (no setting of its own: it follows the comma's).
    Half,
    Sentence,
    Paragraph,
}

impl Pause {
    /// Display time as a percentage of the base word time.
    pub const fn weight(self, p: &Pacing) -> u32 {
        match self {
            Pause::Normal => 100,
            Pause::Clause => p.comma as u32 * 10,
            // A quarter of a word more than a comma (1.75× by default),
            // never past a sentence's end.
            Pause::Half => (p.comma as u32 * 10 + 25).min(p.sentence as u32 * 10),
            Pause::Sentence => p.sentence as u32 * 10,
            Pause::Paragraph => p.paragraph as u32 * 10,
        }
    }
}

/// Text style bits carried from the markup to the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style(u8);

impl Style {
    pub const PLAIN: Style = Style(0);
    pub const BOLD: Style = Style(1);
    pub const ITALIC: Style = Style(2);
    pub const HEADING: Style = Style(4);
    /// Inside `{slow}…{/slow}`: shown 1.4× longer.
    pub const SLOW: Style = Style(8);
    /// Last word of a verse line (`{verse}`): a line break on the page.
    pub const LINE_END: Style = Style(16);
    /// Inside ( … ): speed modes can show side marks instead of the brackets.
    pub const BRACKET: Style = Style(128);

    pub const fn bold(self) -> bool {
        self.0 & (Self::BOLD.0 | Self::HEADING.0) != 0
    }
    pub const fn italic(self) -> bool {
        self.0 & Self::ITALIC.0 != 0
    }
    pub const fn heading(self) -> bool {
        self.0 & Self::HEADING.0 != 0
    }
    pub const fn with(self, other: Style, on: bool) -> Style {
        if on { Style(self.0 | other.0) } else { Style(self.0 & !other.0) }
    }
    pub const fn slow(self) -> bool {
        self.0 & Self::SLOW.0 != 0
    }
    pub const fn line_end(self) -> bool {
        self.0 & Self::LINE_END.0 != 0
    }
    pub const fn bracketed(self) -> bool {
        self.0 & Self::BRACKET.0 != 0
    }
    /// Extra beats after this word from `{pause}` / `{pause:N}` (0–3), each
    /// as long as a plain word.
    pub const fn beats(self) -> u32 {
        ((self.0 >> 5) & 0x3) as u32
    }
    pub const fn with_beats(self, n: u32) -> Style {
        let n = if n > 3 { 3 } else { n };
        Style(self.0 & 0x9F | (n as u8) << 5)
    }
}

/// 8 bytes per word, so a 170k-word book's index is ~1.3 MB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Word {
    pub start: u32,
    pub len: u16,
    pub pause: Pause,
    pub style: Style,
}

impl Word {
    /// Percentage of base time this word stays on screen.
    pub const fn weight(&self, p: &Pacing) -> u32 {
        let chars = self.len as u32; // bytes ≈ letters; close enough for pacing
        let long_bonus = if p.long_words && chars > 8 { (chars - 8) * 8 } else { 0 };
        let long_bonus = if long_bonus > 60 { 60 } else { long_bonus };
        let w = self.pause.weight(p) + long_bonus;
        // Titles are the signposts of a book: they stay longer (2× by default).
        let w = if self.style.heading() { w * p.title as u32 / 10 } else { w };
        let w = if self.style.slow() { w * 14 / 10 } else { w };
        w + self.style.beats() * 100
    }
}

/// A picture, ready for the screen: RGB565 pixels, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: u16,
    pub h: u16,
    pub px: Vec<u16>,
}

/// Marks an image word's text; the rest of the text is its index.
const IMAGE_MARK: char = '\u{FFFC}';
/// Separates text from the image section in a book file (version 1: the
/// pictures one after the other).
pub const IMAGES_MAGIC: &[u8] = b"\n\0FRIMG1";
/// Version 2 adds a table of where each picture starts, so a reader can
/// fetch one picture from storage without reading the rest:
/// count (u16), 0 (u16), then per picture width, height (u16) and offset
/// (u32, from the start of the section after the magic), then the pixels.
pub const IMAGES_MAGIC2: &[u8] = b"\n\0FRIMG2";

/// Pictures that stay in storage (an SD card) until shown.
pub trait PictureSource {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Width and height, without loading the pixels.
    fn dims(&self, n: usize) -> Option<(u16, u16)>;
    fn load(&self, n: usize) -> Option<Image>;
}

/// A book's pictures: in memory, or fetched from storage when shown.
pub enum Pictures {
    Memory(Vec<Image>),
    Stored(alloc::boxed::Box<dyn PictureSource + Send>),
}

impl Pictures {
    pub fn len(&self) -> usize {
        match self {
            Pictures::Memory(v) => v.len(),
            Pictures::Stored(s) => s.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn dims(&self, n: usize) -> Option<(u16, u16)> {
        match self {
            Pictures::Memory(v) => v.get(n).map(|i| (i.w, i.h)),
            Pictures::Stored(s) => s.dims(n),
        }
    }
    pub fn get(&self, n: usize) -> Option<alloc::borrow::Cow<'_, Image>> {
        match self {
            Pictures::Memory(v) => v.get(n).map(alloc::borrow::Cow::Borrowed),
            Pictures::Stored(s) => s.load(n).map(alloc::borrow::Cow::Owned),
        }
    }
}

/// Where the text ends and the image section (after its magic) begins, and
/// whether that section is version 2.
pub fn split_images(bytes: &[u8]) -> (usize, Option<(usize, bool)>) {
    let find = |m: &[u8]| bytes.windows(m.len()).position(|w| w == m);
    match (find(IMAGES_MAGIC2), find(IMAGES_MAGIC)) {
        (Some(i), _) => (i, Some((i + IMAGES_MAGIC2.len(), true))),
        (None, Some(i)) => (i, Some((i + IMAGES_MAGIC.len(), false))),
        _ => (bytes.len(), None),
    }
}

/// A version-2 section's table: (width, height, offset) per picture. Needs
/// only the first `4 + 8 × count` bytes of the section.
pub fn image_table(b: &[u8]) -> Vec<(u16, u16, u32)> {
    if b.len() < 4 {
        return Vec::new();
    }
    let n = u16::from_le_bytes([b[0], b[1]]) as usize;
    (0..n)
        .map_while(|k| {
            let e = b.get(4 + k * 8..12 + k * 8)?;
            Some((u16::from_le_bytes([e[0], e[1]]), u16::from_le_bytes([e[2], e[3]]), u32::from_le_bytes([e[4], e[5], e[6], e[7]])))
        })
        .collect()
}

/// Reads a whole version-2 section held in memory.
pub fn parse_images2(b: &[u8]) -> Vec<Image> {
    image_table(b)
        .into_iter()
        .map_while(|(w, h, off)| {
            let len = w as usize * h as usize * 2;
            let px = b.get(off as usize..off as usize + len)?;
            Some(Image { w, h, px: px.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect() })
        })
        .collect()
}

/// Reads the image section: count (u16), then width, height (u16) and
/// width × height RGB565 pixels (little endian) per image.
pub fn parse_images(mut b: &[u8]) -> Vec<Image> {
    let mut out = Vec::new();
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    if b.len() < 2 {
        return out;
    }
    let n = u16_at(b, 0);
    b = &b[2..];
    for _ in 0..n {
        if b.len() < 4 {
            break;
        }
        let (w, h) = (u16_at(b, 0), u16_at(b, 2));
        let len = w as usize * h as usize * 2;
        if b.len() < 4 + len {
            break;
        }
        let px = b[4..4 + len].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        out.push(Image { w, h, px });
        b = &b[4 + len..];
    }
    out
}

pub struct Book {
    pub title: String,
    /// From an `<!-- author: ... -->` line; empty when the book has none.
    pub author: String,
    /// A page shown in a book's place while it can't be read (its SD card
    /// is out): reading it never moves the book's saved place.
    pub stand_in: bool,
    /// Figure references: (word, the picture word it points to), by word.
    pub figure_refs: Vec<(u32, u32)>,
    /// Lists and tables, in reading order.
    pub lists: Vec<List>,
    pub tables: Vec<Table>,
    /// Pictures referenced by `{image:N}` marks in the text.
    pub images: Pictures,
    /// Descriptions of pictures, by the picture's word index, in order.
    captions: Vec<(u32, String)>,
    /// All words, cleaned of markup, in 64 KB blocks.
    text: ChunkedText,
    words: Chunked<Word>,
    /// Summed weight of words before every `CHECKPOINT`-th word: "time left"
    /// costs at most 63 additions and the table is 1/64 the size of the book.
    cum: Vec<u32>,
    pacing: Pacing,
    /// (first word, title) of every heading, for the Chapters menu.
    chapters: Vec<(u32, String)>,
}

impl Book {
    pub fn new(title: impl Into<String>, source: &str) -> Self {
        let (text, mut words, captions, lists, tables) = tokenize(source);
        let mut chapters: Vec<(u32, String)> = Vec::new();
        let mut i = 0;
        while i < words.len() {
            if words[i].style.heading() {
                let start = i;
                let mut t = String::new();
                while i < words.len() && words[i].style.heading() {
                    if !t.is_empty() {
                        t.push(' ');
                    }
                    let w = words[i];
                    t.push_str(text.get(w.start, w.len as usize));
                    i += 1;
                    if w.pause == Pause::Paragraph {
                        break;
                    }
                }
                chapters.push((start as u32, t));
            } else {
                i += 1;
            }
        }
        mark_asides(&text, &mut words);
        let mut b = Book { title: title.into(), author: String::new(), stand_in: false, lists, tables, figure_refs: Vec::new(), images: Pictures::Memory(Vec::new()), captions, text, words, cum: Vec::new(), chapters, pacing: Pacing::default() };
        b.figure_refs = b.find_figure_refs();
        b.set_pacing(&Pacing::default());
        b
    }

    /// "see figure 11", "(fig. 3)": the words of each reference to a
    /// numbered figure, with that figure's picture. A figure's number comes
    /// from its caption ("Figure 4: Life cycle…", a paragraph starting so,
    /// right after its picture).
    fn find_figure_refs(&self) -> Vec<(u32, u32)> {
        let n = self.words.len();
        let plain = |i: usize| -> String {
            let w = self.words[i];
            self.text_of(w).trim_matches(|c: char| c == '(' || c == ')' || c == '*' || c == '[' || c == ']').to_lowercase()
        };
        let is_fig = |s: &str| matches!(s, "figure" | "figures" | "fig" | "fig." | "figs" | "figs." | "figura" | "figure:" | "fig:");
        let number = |s: &str| -> Option<u32> {
            let d: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            d.parse().ok()
        };
        // Captions: number -> picture word.
        let mut figures: Vec<(u32, u32)> = Vec::new();
        for i in 0..n.saturating_sub(1) {
            let starts_para = i == 0 || self.words[i - 1].pause == Pause::Paragraph;
            if !starts_para || !is_fig(&plain(i)) {
                continue;
            }
            let next = plain(i + 1);
            let Some(num) = number(&next) else { continue };
            if !next.ends_with([':', '.']) && !next.ends_with(|c: char| c.is_ascii_digit()) {
                continue;
            }
            // Its picture: the nearest one before the caption (within about
            // a page; PDFs put text between a drawing and its caption), not
            // already another figure's; else one just after it.
            let free = |k: usize| self.is_image(self.words[k]) && !figures.iter().any(|f| f.1 as usize == k);
            // Several pictures in a row, then their captions: in order (the
            // first caption is the first picture's).
            let pic = (i.saturating_sub(250)..i).rev().find(|&k| free(k)).map(|mut k| {
                while k > 0 && self.is_image(self.words[k - 1]) && free(k - 1) {
                    k -= 1;
                }
                k
            });
            let pic = pic.or_else(|| (i + 2..(i + 40).min(n)).find(|&k| free(k)));
            if let (Some(pic), false) = (pic, figures.iter().any(|f| f.0 == num)) {
                figures.push((num, pic as u32));
            }
        }
        // Descriptions kept with a picture (EPUB <figcaption>): "Figure 3. …".
        for (pic, text) in &self.captions {
            let mut t = text.split_whitespace().map(|w| w.trim_matches('*').to_lowercase());
            if let (Some(f), Some(nb)) = (t.next(), t.next()) {
                if let (true, Some(num)) = (is_fig(&f), number(&nb)) {
                    if !figures.iter().any(|x| x.0 == num) {
                        figures.push((num, *pic));
                    }
                }
            }
        }
        // References: a figure word and its number anywhere else.
        let mut refs = Vec::new();
        for i in 0..n.saturating_sub(1) {
            if !is_fig(&plain(i)) {
                continue;
            }
            let starts_para = i == 0 || self.words[i - 1].pause == Pause::Paragraph;
            let Some(num) = number(&plain(i + 1)) else { continue };
            if starts_para && number(&plain(i + 1)).is_some() && plain(i + 1).ends_with([':', '.']) {
                continue; // a caption ("Figure 4: …"), not a reference
            }
            if let Some(&(_, pic)) = figures.iter().find(|f| f.0 == num) {
                refs.push((i as u32, pic));
                refs.push((i as u32 + 1, pic));
            }
        }
        refs
    }

    /// Word `i` ends a reference to a figure (the "11" of "figure 11").
    pub fn ends_figure_ref(&self, i: usize) -> bool {
        self.figure_ref(i).is_some() && self.figure_ref(i + 1).is_none()
    }

    /// The picture word a reference at word `i` points to ("figure 11").
    pub fn figure_ref(&self, i: usize) -> Option<usize> {
        let i = i as u32;
        self.figure_refs.binary_search_by_key(&i, |r| r.0).ok().map(|k| self.figure_refs[k].1 as usize)
    }

    /// Builds a book from a file's bytes: text, then optionally the image
    /// section (see [`IMAGES_MAGIC`]).
    pub fn from_bytes(bytes: &[u8], fallback: &str) -> Self {
        let (end, section) = split_images(bytes);
        let images = match section {
            Some((at, true)) => parse_images2(&bytes[at..]),
            Some((at, false)) => parse_images(&bytes[at..]),
            None => Vec::new(),
        };
        let mut book = Book::parse(&String::from_utf8_lossy(&bytes[..end]), fallback);
        book.images = Pictures::Memory(images);
        book
    }

    /// The description of the picture at word `i`, if it has one.
    pub fn caption(&self, i: usize) -> Option<&str> {
        let k = self.captions.binary_search_by_key(&(i as u32), |c| c.0).ok()?;
        Some(&self.captions[k].1)
    }

    /// Which picture an image word stands for.
    pub fn picture_index(&self, w: Word) -> Option<usize> {
        self.text_of(w).strip_prefix(IMAGE_MARK)?.parse().ok()
    }

    /// The picture a word stands for, if it is an image word (loaded from
    /// storage if the book keeps its pictures there).
    pub fn image_of(&self, w: Word) -> Option<alloc::borrow::Cow<'_, Image>> {
        self.images.get(self.picture_index(w)?)
    }

    /// A picture's size without loading it (for laying out pages).
    pub fn image_dims(&self, w: Word) -> Option<(u16, u16)> {
        self.images.dims(self.picture_index(w)?)
    }

    /// Whether a word is an image (even if its picture is missing).
    pub fn is_image(&self, w: Word) -> bool {
        self.text_of(w).starts_with(IMAGE_MARK)
    }

    /// Builds a book from converter output: an optional first line
    /// `<!-- title: ... -->` names it, otherwise `fallback` does; an
    /// `<!-- author: ... -->` line may follow.
    pub fn parse(source: &str, fallback: &str) -> Self {
        let (title, body) = match source.strip_prefix("<!-- title:").and_then(|r| r.split_once("-->")) {
            Some((t, rest)) => (t.trim(), rest),
            None => (fallback, source),
        };
        let (author, body) = match body.trim_start_matches(['\n', '\r']).strip_prefix("<!-- author:").and_then(|r| r.split_once("-->")) {
            Some((a, rest)) => (a.trim(), rest),
            None => ("", body),
        };
        let mut b = Book::new(title, body);
        b.author = author.into();
        b
    }

    /// The list word `i` is in, and which item.
    pub fn list_at(&self, i: usize) -> Option<(&List, usize)> {
        let i = i as u32;
        let l = self.lists.iter().find(|l| l.items[0].0 <= i && i < l.end)?;
        let item = l.items.iter().rposition(|(w, _)| *w <= i)?;
        Some((l, item))
    }

    /// The table word `i` is in, and which cell (index into `cells`).
    pub fn cell_at(&self, i: usize) -> Option<(&Table, usize)> {
        let i = i as u32;
        let t = self.tables.iter().find(|t| t.cells[0].0 <= i && i < t.end)?;
        let cell = t.cells.iter().rposition(|(w, _, _)| *w <= i)?;
        Some((t, cell))
    }

    /// Word `i` is the first of a table cell (the grid flashes before it).
    pub fn starts_cell(&self, i: usize) -> bool {
        self.cell_at(i).is_some_and(|(t, c)| t.cells[c].0 as usize == i)
    }

    /// Recomputes the running time totals for new pause settings.
    pub fn set_pacing(&mut self, p: &Pacing) {
        self.pacing = *p;
        self.cum.clear();
        self.cum.reserve(self.words.len() / CHECKPOINT + 2);
        let mut acc = 0u32;
        for (i, w) in self.words.iter().enumerate() {
            if i % CHECKPOINT == 0 {
                self.cum.push(acc);
            }
            acc += self.weight_at(i, w, p);
        }
        self.cum.push(acc); // total, at the end
        self.cum.shrink_to_fit();
    }

    /// Summed weight of words before `i`.
    fn weight_before(&self, i: usize) -> u32 {
        let i = i.min(self.words.len());
        if i == self.words.len() {
            return *self.cum.last().unwrap_or(&0);
        }
        let c = i / CHECKPOINT;
        let p = &self.pacing;
        self.cum[c] + (c * CHECKPOINT..i).zip(self.words.range(c * CHECKPOINT..i)).map(|(k, w)| self.weight_at(k, w, p)).sum::<u32>()
    }

    /// Word `i`'s time: inside a table read at half speed; with the table
    /// flashing before each cell, a cell's first word has a beat more (the
    /// grid shows for that beat, then the word for its own time).
    fn weight_at(&self, i: usize, w: &Word, p: &Pacing) -> u32 {
        let n = i as u32;
        match self.tables.iter().find(|t| t.cells[0].0 <= n && n < t.end) {
            Some(t) => w.weight(p) * 2 + if p.table_flash && t.cells.iter().any(|c| c.0 == n) { 100 } else { 0 },
            None => w.weight(p),
        }
    }

    /// A table cell's words as one line of text (for drawing the table).
    pub fn cell_text(&self, t: &Table, cell: usize) -> String {
        let start = t.cells[cell].0 as usize;
        let end = t.cells.get(cell + 1).map_or(t.end, |c| c.0) as usize;
        let mut s = String::new();
        for i in start..end.max(start) {
            if let Some(w) = self.word(i) {
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(self.text_of(w));
            }
        }
        s
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn word(&self, i: usize) -> Option<Word> {
        self.words.get(i)
    }

    pub fn text_of(&self, w: Word) -> &str {
        self.text.get(w.start, w.len as usize)
    }

    /// Summed weight from word `i` to the end.
    pub fn weight_from(&self, i: usize) -> u32 {
        self.weight_before(self.words.len()) - self.weight_before(i)
    }

    /// First word of the sentence containing `i`.
    pub fn sentence_start(&self, i: usize) -> usize {
        // A table is one sentence (⏮ ⏭ jump over it whole; ‹ › go by cell).
        if let Some((t, _)) = self.cell_at(i) {
            return t.cells[0].0 as usize;
        }
        let mut j = i.min(self.words.len().saturating_sub(1));
        while j > 0 && self.words[j - 1].pause < Pause::Sentence {
            j -= 1;
        }
        j
    }

    /// First word of the sentence after the one containing `i`, if any.
    pub fn next_sentence(&self, i: usize) -> Option<usize> {
        if let Some((t, _)) = self.cell_at(i) {
            return ((t.end as usize) < self.words.len()).then_some(t.end as usize);
        }
        let mut j = i;
        while j < self.words.len() {
            if self.words[j].pause >= Pause::Sentence {
                return (j + 1 < self.words.len()).then_some(j + 1);
            }
            j += 1;
        }
        None
    }

    /// First word of the paragraph containing `i`.
    pub fn paragraph_start(&self, i: usize) -> usize {
        let mut j = i.min(self.words.len().saturating_sub(1));
        while j > 0 && self.words[j - 1].pause < Pause::Paragraph {
            j -= 1;
        }
        j
    }

    /// First word of the next paragraph, if any.
    pub fn next_paragraph(&self, i: usize) -> Option<usize> {
        (i..self.words.len()).find(|&j| self.words[j].pause == Pause::Paragraph).map(|j| j + 1).filter(|&j| j < self.words.len())
    }

    pub fn chapters(&self) -> &[(u32, String)] {
        &self.chapters
    }

    /// Index into [`Book::chapters`] of the chapter containing word `i`.
    pub fn chapter_index(&self, i: usize) -> Option<usize> {
        self.chapters.iter().rposition(|(start, _)| *start as usize <= i)
    }

    /// First word of the chapter containing `i`, if inside one.
    pub fn chapter_at(&self, i: usize) -> Option<usize> {
        self.chapter_index(i).map(|c| self.chapters[c].0 as usize)
    }

    /// Words shown together when flashing `n` at a time, starting at `i`.
    /// A group never runs past punctuation or out of a heading, so phrases
    /// stay intact.
    pub fn group(&self, i: usize, n: u8) -> core::ops::Range<usize> {
        let mut end = (i + 1).min(self.words.len());
        while end < self.words.len()
            && end - i < n as usize
            && self.words[end - 1].pause == Pause::Normal
            && self.words[end].style.heading() == self.words[i].style.heading()
        {
            end += 1;
        }
        i..end.max(i)
    }

    /// A phrase group starting at `i` for Phrase and Page modes, the way the
    /// original e-reader builds them: `n` words in reading order, ending
    /// early only at a sentence or paragraph end (commas don't split a
    /// group, they only make it stay longer). Chapter titles are never
    /// mixed with body text.
    pub fn phrase(&self, i: usize, n: u8) -> core::ops::Range<usize> {
        let len = self.words.len();
        if i >= len {
            return i..i;
        }
        let mut end = i + 1;
        while end < len
            && end - i < n.max(1) as usize
            && self.words[end - 1].pause < Pause::Sentence
            && self.words[end].style.heading() == self.words[i].style.heading()
        {
            end += 1;
        }
        i..end
    }

    /// Summed weight of a range of words.
    pub fn weight_of(&self, r: core::ops::Range<usize>) -> u32 {
        let e = r.end.min(self.words.len());
        let s = r.start.min(e);
        let p = &self.pacing;
        (s..e).zip(self.words.range(s..e)).map(|(k, w)| self.weight_at(k, w, p)).sum()
    }

    /// Writes the book as HTML for the phone/computer web view. Every word is
    /// a `<span id="wN">` so the page can highlight the word being read.
    /// Streams into any `fmt::Write`, so the ESP32 can send it in chunks.
    pub fn write_html(&self, out: &mut impl Write) -> fmt::Result {
        self.write_html_range(0..self.words.len(), out)
    }

    /// The whole paragraphs covering roughly `from..to`, for loading a big
    /// book into the web view a piece at a time.
    pub fn paragraphs_around(&self, from: usize, to: usize) -> core::ops::Range<usize> {
        let n = self.words.len();
        if n == 0 {
            return 0..0;
        }
        let a = self.paragraph_start(from.min(n - 1));
        let z = if to >= n { n } else { self.next_paragraph(to.saturating_sub(1).max(a)).unwrap_or(n) };
        a..z.max(a + 1).min(n)
    }

    /// HTML for words `r` (call with whole paragraphs, see `paragraphs_around`).
    pub fn write_html_range(&self, r: core::ops::Range<usize>, out: &mut impl Write) -> fmt::Result {
        let mut open = false;
        for i in r.start..r.end.min(self.words.len()) {
            let w = self.words[i];
            if !open {
                out.write_str(if w.style.heading() { "<h2>" } else { "<p>" })?;
                open = true;
            }
            let (b, it) = (w.style.bold() && !w.style.heading(), w.style.italic());
            if self.is_image(w) {
                let n = &self.text_of(w)[IMAGE_MARK.len_utf8()..];
                match self.caption(i) {
                    Some(c) => {
                        write!(out, "<figure id=\"w{i}\"><img src=\"/img/{n}\" alt=\"\"><figcaption>")?;
                        escape_html(c, out)?;
                        out.write_str("</figcaption></figure>")?;
                    }
                    None => write!(out, "<span id=\"w{i}\"><img src=\"/img/{n}\" alt=\"\"></span>")?,
                }
                out.write_str(if open { "</p>\n" } else { "\n" })?;
                open = false;
                continue;
            }
            // A list item starts on its own line with its marker; a table
            // row on its own line, its cells apart.
            let fresh = i == r.start || self.words.get(i - 1).is_none_or(|p| p.pause == Pause::Paragraph);
            if let Some((l, k)) = self.list_at(i).filter(|(l, k)| l.items[*k].0 as usize == i) {
                if !fresh {
                    out.write_str("<br>\n")?;
                }
                let m = if l.numbered() { l.items[k].1.as_str() } else { ["•", "◦", "▪", "▪"][l.levels.get(k).copied().unwrap_or(0).min(3) as usize] };
                let lv = l.levels.get(k).copied().unwrap_or(0);
                if lv > 0 {
                    write!(out, "<span class=\"mk\" style=\"margin-left:{}em\">{m} </span>", lv as f32 * 1.4)?;
                } else {
                    write!(out, "<span class=\"mk\">{m} </span>")?;
                }
            } else if let Some((t, c)) = self.cell_at(i).filter(|(t, c)| t.cells[*c].0 as usize == i) {
                if t.cells[c].2 == 0 {
                    if !fresh {
                        out.write_str("<br>\n")?;
                    }
                } else {
                    out.write_str("<span class=\"mk\">· </span>")?;
                }
            }
            write!(out, "<span id=\"w{i}\">")?;
            if b { out.write_str("<b>")?; }
            if it { out.write_str("<i>")?; }
            escape_html(self.text_of(w), out)?;
            if it { out.write_str("</i>")?; }
            if b { out.write_str("</b>")?; }
            out.write_str("</span>")?;
            let heading_ends = w.style.heading()
                && self.words.get(i + 1).is_none_or(|n| !n.style.heading() || w.pause == Pause::Paragraph);
            if w.pause == Pause::Paragraph || heading_ends {
                out.write_str(if w.style.heading() { "</h2>\n" } else { "</p>\n" })?;
                open = false;
            } else if w.style.line_end() {
                out.write_str("<br>\n")?;
            } else {
                out.write_char(' ')?;
            }
        }
        Ok(())
    }
}

fn escape_html(s: &str, out: &mut impl Write) -> fmt::Result {
    for c in s.chars() {
        match c {
            '<' => out.write_str("&lt;")?,
            '>' => out.write_str("&gt;")?,
            '&' => out.write_str("&amp;")?,
            '"' => out.write_str("&quot;")?,
            _ => out.write_char(c)?,
        }
    }
    Ok(())
}

const TRAILING_PUNCT: &[char] = &['.', ',', ';', ':', '!', '?', '…', '"', '\'', ')', ']', '»', '”', '’', '—', '–'];

/// Pictures' descriptions: (word index of the picture, text).
type Captions = Vec<(u32, String)>;

/// A list: each item's first word and its marker as printed ("•", "1.",
/// "a)"), and the word after the last item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    pub items: Vec<(u32, String)>,
    /// Each item's nesting level (0 = top), from its indentation.
    pub levels: Vec<u8>,
    pub end: u32,
}

impl List {
    /// The deepest nesting level in the list.
    pub fn depth(&self) -> u8 {
        self.levels.iter().copied().max().unwrap_or(0)
    }

    /// Numbered ("1.", "a)", "iv.") rather than bulleted.
    pub fn numbered(&self) -> bool {
        self.items.first().is_some_and(|(_, m)| m.chars().any(|c| c.is_alphanumeric()))
    }
}

/// A table, read row by row: its size, how many rows are headers, and each
/// cell's first word (row, column). Empty cells have no words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub rows: u16,
    pub cols: u16,
    pub header: u16,
    pub cells: Vec<(u32, u16, u16)>,
    pub end: u32,
}

/// A list item's marker, if `tok` (the first on a line) is one: bullets
/// "•", "*", "+", "◦", "▪", "‣"; numbers and letters "1." "1)" "(1)" "a."
/// "b)" "iv."; not "-" or "–", which start dialogue in many books.
fn item_marker(tok: &str) -> Option<&str> {
    if matches!(tok, "•" | "*" | "+" | "◦" | "▪" | "‣" | "●" | "○" | "■" | "□") {
        return Some(tok);
    }
    let inner = tok.strip_prefix('(').map_or(tok, |t| t);
    let body = inner.strip_suffix(['.', ')'])?;
    let roman = |b: &str| !b.is_empty() && b.len() <= 4 && b.bytes().all(|c| b"ivx".contains(&c));
    let ok = (1..=3).contains(&body.len()) && body.bytes().all(|c| c.is_ascii_digit())
        || body.len() == 1 && body.bytes().all(|c| c.is_ascii_lowercase())
        || roman(body);
    ok.then_some(tok)
}

/// A table row: `| a | b |`. Returns its cells, or None. A separator row
/// (`|---|:---:|`) gives Some(empty).
fn table_row(line: &str) -> Option<Vec<&str>> {
    let t = line.trim();
    if !(t.len() >= 3 && t.starts_with('|') && t.ends_with('|')) {
        return None;
    }
    let cells: Vec<&str> = t[1..t.len() - 1].split('|').map(str::trim).collect();
    if cells.iter().all(|c| !c.is_empty() && c.trim_matches(':').bytes().all(|b| b == b'-') && c.contains("--")) {
        return Some(Vec::new());
    }
    Some(cells)
}

fn tokenize(source: &str) -> (ChunkedText, Chunked<Word>, Captions, Vec<List>, Vec<Table>) {
    let mut text = ChunkedText::default();
    let mut words: Chunked<Word> = Chunked::new();
    let mut style = Style::PLAIN;
    let mut scratch = String::new();
    let mut verse = false;
    let mut captions: Captions = Vec::new();
    // Inside {caption}…{/caption}: words go to the last picture's description.
    let mut caption: Option<String> = None;
    let mut lists: Vec<List> = Vec::new();
    let mut tables: Vec<Table> = Vec::new();
    // A list or table still open (blank lines don't close a list).
    let mut in_list = false;
    // The indentations seen in the open list: an item's level is how many
    // smaller ones there are (2 or 4 spaces per level both work).
    let mut indents: Vec<usize> = Vec::new();
    // This list puts each item's picture before its caption (a catalogue:
    // picture, "1. title…"): a picture just before an item is that item's.
    let mut pics_first = false;
    let mut in_table = false;
    let mut row_cells: Vec<&str> = Vec::new();
    let mut row_line = String::new();

    for raw in source.lines() {
        // Table rows become one line of cells, each opened by "{cell}".
        let mut line = raw;
        match table_row(raw) {
            Some(cells) if cells.is_empty() => {
                if let Some(t) = tables.last_mut().filter(|_| in_table) {
                    t.header = t.rows; // the rows above are headers
                }
                continue;
            }
            Some(cells) => {
                if !in_table {
                    in_table = true;
                    if let Some(last) = words.last_mut() {
                        last.pause = Pause::Paragraph;
                    }
                    tables.push(Table { rows: 0, cols: 0, header: 0, cells: Vec::new(), end: 0 });
                }
                row_cells.clear();
                row_cells.extend(cells);
                row_line.clear();
                for c in &row_cells {
                    row_line.push_str(" {cell} ");
                    row_line.push_str(c);
                }
                line = row_line.as_str();
            }
            None => {
                if in_table && !raw.trim().is_empty() {
                    in_table = false;
                    if let Some(t) = tables.last_mut() {
                        t.end = words.len() as u32;
                    }
                }
            }
        }
        let row = in_table && table_row(raw).is_some();
        let mut col: u16 = 0;
        let tidy = tidy_dashes(line);
        let line = tidy.as_str();
        let mut tokens = line.split_whitespace().peekable();
        let Some(&first) = tokens.peek() else {
            // Blank line: paragraph break.
            if let Some(last) = words.last_mut() {
                last.pause = Pause::Paragraph;
            }
            continue;
        };
        let heading = (1..=6).contains(&first.len()) && first.bytes().all(|b| b == b'#') && line.trim_start().len() > first.len();
        if heading {
            tokens.next();
        }
        // A line that is only a list number ("1." under a thumbnail in a
        // catalogue's index): a label, nothing to read.
        if !heading && !row && line.split_whitespace().count() == 1 && item_marker(first).is_some_and(|m| m.chars().any(|c| c.is_alphanumeric())) {
            continue;
        }
        // A list item: its marker is shown beside the word, not read.
        let marker = (!heading && !row).then(|| item_marker(first)).flatten().filter(|_| line.split_whitespace().nth(1).is_some());
        match marker {
            Some(m) => {
                tokens.next();
                let is_pic = |words: &Chunked<Word>, i: usize| words.get(i).is_some_and(|w| text.get(w.start, w.len as usize).starts_with(IMAGE_MARK));
                if !in_list {
                    in_list = true;
                    indents.clear();
                    pics_first = !words.is_empty() && is_pic(&words, words.len() - 1);
                    if let Some(last) = words.last_mut() {
                        last.pause = Pause::Paragraph;
                    }
                    lists.push(List { items: Vec::new(), levels: Vec::new(), end: 0 });
                } else if let Some(last) = words.last_mut() {
                    // Between items: at least a sentence's breath.
                    last.pause = last.pause.max(Pause::Sentence);
                }
                // Picture-first lists: the pictures just before this item
                // are its own (never the previous item's first word).
                let floor = lists.last().unwrap().items.last().map_or(0, |(w, _)| *w as usize + 1);
                let mut first = words.len();
                while pics_first && first > floor && is_pic(&words, first - 1) {
                    first -= 1;
                }
                let indent: usize = raw.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 4 } else { 1 }).sum();
                if !indents.contains(&indent) {
                    indents.push(indent);
                }
                let level = indents.iter().filter(|&&x| x < indent).count().min(3) as u8;
                let l = lists.last_mut().unwrap();
                l.items.push((first as u32, String::from(m)));
                l.levels.push(level);
            }
            // A picture (or its description) inside a list doesn't end it.
            None if in_list && !(first.starts_with("{image:") || first == "{caption}") => {
                in_list = false;
                lists.last_mut().unwrap().end = words.len() as u32;
                if let Some(last) = words.last_mut() {
                    last.pause = Pause::Paragraph;
                }
            }
            None => {}
        }
        let first_word = words.len();

        for tok in tokens {
            if tok == "{cell}" {
                // A table cell starts: the reader flashes the grid first.
                if let Some(t) = tables.last_mut() {
                    if col > 0 && words.len() > first_word {
                        // A breath before the next column.
                        if let Some(last) = words.last_mut() {
                            last.pause = last.pause.max(Pause::Sentence);
                        }
                    }
                    t.cells.push((words.len() as u32, t.rows, col));
                    col += 1;
                    t.cols = t.cols.max(col);
                }
                continue;
            }
            // A picture's description: kept with the picture, not read as words.
            if tok == "{caption}" {
                caption = Some(String::new());
                continue;
            }
            if tok == "{/caption}" {
                if let Some(c) = caption.take() {
                    let pic = (0..words.len()).rev().find(|&i| text.get(words[i].start, words[i].len as usize).starts_with(IMAGE_MARK));
                    match pic {
                        Some(i) if !c.is_empty() => captions.push((i as u32, c)),
                        _ => {}
                    }
                }
                continue;
            }
            if let Some(c) = caption.as_mut() {
                if !c.is_empty() {
                    c.push(' ');
                }
                c.push_str(tok);
                continue;
            }
            // Timing marks (SSML-like): they shape pauses, they aren't words.
            match tok {
                "{slow}" | "{/slow}" => {
                    style = style.with(Style::SLOW, tok == "{slow}");
                    continue;
                }
                "{verse}" | "{/verse}" => {
                    verse = tok == "{verse}";
                    continue;
                }
                _ => {}
            }
            if let Some(n) = tok.strip_prefix("{image:").and_then(|r| r.strip_suffix('}')).and_then(|r| r.parse::<u32>().ok()) {
                // A picture is its own paragraph, shown 4× a plain word (paragraph + 1 beat).
                if let Some(last) = words.last_mut() {
                    last.pause = Pause::Paragraph;
                }
                scratch.clear();
                scratch.push(IMAGE_MARK);
                let _ = core::fmt::Write::write_fmt(&mut scratch, format_args!("{n}"));
                words.push(Word { start: text.push(&scratch), len: scratch.len() as u16, pause: Pause::Paragraph, style: Style::PLAIN.with_beats(1) });
                continue;
            }
            if let Some(n) = tok.strip_prefix("{pause").and_then(|r| r.strip_suffix('}')).and_then(|r| if r.is_empty() { Some(1) } else { r.strip_prefix(':')?.parse::<u32>().ok() }) {
                if let Some(last) = words.last_mut() {
                    last.style = last.style.with_beats(last.style.beats() + n);
                }
                continue;
            }
            let (lead, core, trail_markers, punct) = split_markers(tok);
            if core.is_empty() && punct.is_empty() {
                continue; // a lone `*` bullet or `---` rule: not a word, not a toggle
            }
            for m in lead {
                style = toggle(style, m);
            }
            scratch.clear();
            scratch.push_str(core);
            scratch.push_str(punct);
            // Very long tokens (URLs, junk) are cut to fit one text block.
            let mut cut = scratch.len().min(4096);
            while !scratch.is_char_boundary(cut) {
                cut -= 1;
            }
            let shown = &scratch[..cut];
            let closes = shown.chars().any(|c| CLOSERS.contains(&c));
            // Which words are inside brackets is worked out once the whole
            // text is read (`mark_asides`): an aside may cross a page break.
            let st = style.with(Style::HEADING, heading);
            words.push(Word {
                start: text.push(shown),
                len: shown.len() as u16,
                pause: classify(shown),
                style: st,
            });
            // Brackets are punctuation: a breath as the aside opens and closes.
            if shown.starts_with(OPENERS) && words.len() >= 2 {
                let n = words.len();
                let before = words.get(n - 2).unwrap();
                if before.pause < Pause::Clause {
                    words.set(n - 2, Word { pause: Pause::Clause, ..before });
                }
            }
            if closes && shown.trim_end_matches(['.', ',', ';', ':', '!', '?', '"', '\'', '”', '’']).ends_with(CLOSERS) {
                let n = words.len();
                let me = words.get(n - 1).unwrap();
                if me.pause < Pause::Clause {
                    words.set(n - 1, Word { pause: Pause::Clause, ..me });
                }
            }
            for m in trail_markers {
                style = toggle(style, m);
            }
        }
        if heading && words.len() > first_word {
            words.last_mut().unwrap().pause = Pause::Paragraph;
        }
        if row {
            // A row ends with a paragraph's breath.
            if let Some(t) = tables.last_mut() {
                t.rows += 1;
            }
            if let Some(last) = words.last_mut() {
                last.pause = last.pause.max(Pause::Paragraph);
            }
        }
        // In verse, every line break is a break on the page and a breath.
        if verse && !heading && words.len() > first_word {
            let last = words.last_mut().unwrap();
            last.style = last.style.with(Style::LINE_END, true);
            last.pause = last.pause.max(Pause::Clause);
        }
    }
    if let Some(last) = words.last_mut() {
        last.pause = Pause::Paragraph;
    }
    if in_list {
        lists.last_mut().unwrap().end = words.len() as u32;
    }
    if in_table {
        tables.last_mut().unwrap().end = words.len() as u32;
    }
    lists.retain(|l| !l.items.is_empty());
    tables.retain(|t| !t.cells.is_empty());
    text.shrink_to_fit();
    words.shrink_to_fit();
    (text, words, captions, lists, tables)
}

#[derive(Clone, Copy)]
enum Marker {
    Bold,
    Italic,
}

fn toggle(s: Style, m: Marker) -> Style {
    match m {
        Marker::Bold => Style(s.0 ^ Style::BOLD.0),
        Marker::Italic => s.with(Style::ITALIC, !s.italic()),
    }
}

/// Splits `**word**.` into leading markers, `word`, trailing markers and `.`.
fn split_markers(tok: &str) -> (MarkerIter<'_>, &str, MarkerIter<'_>, &str) {
    let lead_len = tok.len() - tok.trim_start_matches(['*', '_']).len();
    let rest = &tok[lead_len..];
    let no_punct = rest.trim_end_matches(TRAILING_PUNCT);
    let punct = &rest[no_punct.len()..];
    let core = no_punct.trim_end_matches(['*', '_']);
    let trail = &no_punct[core.len()..];
    if core.is_empty() {
        // Only markers and punctuation: treat the punctuation as the word.
        return (MarkerIter(""), "", MarkerIter(""), punct);
    }
    (MarkerIter(&tok[..lead_len]), core, MarkerIter(trail), punct)
}

/// Yields markers from a run like `**_`: `**` is bold, single `*`/`_` italic.
struct MarkerIter<'a>(&'a str);

impl Iterator for MarkerIter<'_> {
    type Item = Marker;
    fn next(&mut self) -> Option<Marker> {
        if let Some(r) = self.0.strip_prefix("**").or_else(|| self.0.strip_prefix("__")) {
            self.0 = r;
            Some(Marker::Bold)
        } else if !self.0.is_empty() {
            self.0 = &self.0[1..];
            Some(Marker::Italic)
        } else {
            None
        }
    }
}

/// Brackets that open and close an aside: round ( ), square [ ], curly { },
/// angle < > (also << >>), and guillemets « » ‹ ›.
pub const OPENERS: [char; 6] = ['(', '[', '{', '<', '«', '‹'];
pub const CLOSERS: [char; 6] = [')', ']', '}', '>', '»', '›'];

/// Longest aside (in words) a bracket pair may span; past that, a "closer"
/// is more likely a stray (a list's "a)") than the end of this aside.
const MAX_ASIDE: usize = 1500;

/// Flags every word inside brackets. Each closer is paired with the latest
/// opener of its own kind, across paragraph breaks (books and PDF page
/// breaks split asides). Openers never closed reach only to the end of
/// their paragraph; closers without an opener ("1)", "a)") are ignored.
fn mark_asides(text: &crate::chunked::ChunkedText, words: &mut Chunked<Word>) {
    let kind = |c: char, set: &[char; 6]| set.iter().position(|&k| k == c);
    let mut open: Vec<(usize, usize)> = Vec::new(); // (kind, word)
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for i in 0..words.len() {
        let w = words.get(i).unwrap();
        for c in text.get(w.start, w.len as usize).chars() {
            if let Some(k) = kind(c, &OPENERS) {
                open.push((k, i));
            } else if let Some(k) = kind(c, &CLOSERS) {
                if let Some(at) = open.iter().rposition(|&(ok, _)| ok == k) {
                    let start = open[at].1;
                    // Openers left inside were never closed: they end here too.
                    open.truncate(at);
                    if i - start <= MAX_ASIDE {
                        spans.push((start, i));
                    }
                }
            }
        }
    }
    for (_, start) in open {
        let mut end = start;
        while end + 1 < words.len() && words.get(end).unwrap().pause != Pause::Paragraph {
            end += 1;
        }
        spans.push((start, end));
    }
    for (a, z) in spans {
        for i in a..=z {
            let w = words.get(i).unwrap();
            words.set(i, Word { style: w.style.with(Style::BRACKET, true), ..w });
        }
    }
}

/// The bracket pair the aside around word `i` uses, found by walking back
/// to its opener (doubled ASCII << >> shows as « »).
pub fn bracket_pair(book: &Book, i: usize) -> (&'static str, &'static str) {
    let mut depth = 0i32;
    let mut j = i + 1;
    while j > 0 && i + 1 - j < 400 {
        j -= 1;
        let Some(w) = book.word(j) else { break };
        if !w.style.bracketed() && j != i {
            break;
        }
        let t = book.text_of(w);
        for c in t.chars().rev() {
            if CLOSERS.contains(&c) && j != i {
                depth += 1;
            } else if OPENERS.contains(&c) {
                if depth == 0 {
                    return match c {
                        '<' if t.contains("<<") => ("«", "»"),
                        '<' => ("<", ">"),
                        '«' => ("«", "»"),
                        '‹' => ("‹", "›"),
                        '[' => ("[", "]"),
                        '{' => ("{", "}"),
                        _ => ("(", ")"),
                    };
                }
                depth -= 1;
            }
        }
    }
    ("(", ")")
}

/// Readies a line for splitting into words: drops the invisible
/// characters some books use to steer line breaks (word joiner, zero-width
/// spaces, soft hyphen), and breaks after an em dash between two words
/// ("unless—unless" reads "unless—", "unless": the dash stays where the
/// break is). Ranges ("1914–1918"), hyphenated words, a dash opening a line
/// of dialogue and one ending interrupted speech stay as they are.
fn tidy_dashes(line: &str) -> String {
    let chars: Vec<char> = line.chars().filter(|c| !matches!(c, '\u{FEFF}' | '\u{2060}' | '\u{200B}' | '\u{00AD}')).collect();
    let mut out = String::with_capacity(line.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        out.push(c);
        let wordish = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || matches!(c, '’' | '\'' | '”' | '"' | ')' | '*' | '_' | '.' | ',' | '!' | '?'));
        if c == '—' && i > 0 && wordish(chars.get(i - 1)) && chars.get(i + 1).is_some_and(|n| n.is_alphanumeric() || matches!(n, '“' | '‘' | '"' | '(' | '*' | '_')) {
            out.push(' ');
        }
    }
    out
}

fn classify(word: &str) -> Pause {
    // Look past closing quotes/brackets: `end."` still ends a sentence.
    let tail = word.trim_end_matches(['"', '\'', ')', ']', '}', '>', '»', '›', '”', '’']);
    match tail.chars().last() {
        Some('.' | '!' | '?' | '…') => Pause::Sentence,
        Some(';' | ':') => Pause::Half,
        Some(',' | '—' | '–') => Pause::Clause,
        _ => Pause::Normal,
    }
}


/// Optimal Recognition Point: index (in chars) of the letter the eye should
/// land on. Slightly left of centre, the classic Spritz-style table.
pub fn orp_index(word: &str) -> usize {
    // Ignore leading punctuation like quotes when choosing the focus letter.
    let lead = word.chars().take_while(|c| !c.is_alphanumeric()).count();
    let core = word.chars().skip(lead).filter(|c| c.is_alphanumeric()).count();
    if core == 0 {
        return word.chars().count() / 2;
    }
    let idx = match core {
        0..=1 => 0,
        2..=5 => 1,
        6..=9 => 2,
        10..=13 => 3,
        _ => 4,
    };
    // The idx-th *letter*: an apostrophe or hyphen inside the word doesn't
    // count ("c'è" → "è", "un'opera" → "o", "l'arte" → "a").
    let _ = lead;
    word.chars().enumerate().filter(|(_, c)| c.is_alphanumeric()).nth(idx).map_or(0, |(i, _)| i)
}

/// Words over 13 letters shown in two flashes, as Spritz does (patent
/// US9483109): the recognition point then never has to move for a word too
/// long to fit around it. Splits at a hyphen if there is one near the
/// middle, otherwise at a syllable boundary (vowel | consonant + vowel) close
/// to the middle. Returns the two parts as shown ("contempora-", "neamente").
pub fn split_long(word: &str) -> Option<(String, String)> {
    let chars: Vec<char> = word.chars().collect();
    let letters = chars.iter().filter(|c| c.is_alphanumeric()).count();
    if letters <= 13 {
        return None;
    }
    let n = chars.len();
    let mid = n / 2;
    let vowel = |c: char| "aeiouàèéìíòóùúAEIOUÀÈÉÌÒÙäöüÄÖÜy".contains(c);
    let near = |i: &usize| (*i as isize - mid as isize).unsigned_abs();
    if let Some(h) = (n / 4..3 * n / 4).filter(|&i| chars[i] == '-').min_by_key(near) {
        return Some((chars[..=h].iter().collect(), chars[h + 1..].iter().collect()));
    }
    let cut = (n / 3..=2 * n / 3)
        .filter(|&i| i > 0 && i + 1 < n && vowel(chars[i - 1]) && chars[i].is_alphabetic() && !vowel(chars[i]) && vowel(chars[i + 1]))
        .min_by_key(near)
        .unwrap_or(mid);
    let mut first: String = chars[..cut].iter().collect();
    first.push('-');
    Some((first, chars[cut..].iter().collect()))
}

/// Splits a word around its ORP letter: (before, focus, after).
pub fn split_orp(word: &str) -> (&str, &str, &str) {
    let idx = orp_index(word);
    let mut it = word.char_indices().skip(idx);
    match it.next() {
        Some((a, c)) => {
            let b = a + c.len_utf8();
            (&word[..a], &word[a..b], &word[b..])
        }
        None => (word, "", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(b: &Book) -> Vec<&str> {
        (0..b.len()).map(|i| b.text_of(b.word(i).unwrap())).collect()
    }

    fn styles(b: &Book) -> Vec<(bool, bool, bool)> {
        (0..b.len()).map(|i| b.word(i).unwrap().style).map(|s| (s.bold(), s.italic(), s.heading())).collect()
    }

    #[test]
    fn dashes_between_words_split_and_invisible_joiners_go() {
        let b = Book::new("t", "“unless\u{FEFF}—unless we’re one—and two. 1914–1918 well-known —Hola, dijo—");
        let w: Vec<String> = (0..b.len()).map(|i| String::from(b.text_of(b.word(i).unwrap()))).collect();
        assert_eq!(w, ["“unless—", "unless", "we’re", "one—", "and", "two.", "1914–1918", "well-known", "—Hola,", "dijo—"]);
        assert_eq!(b.word(0).unwrap().pause, Pause::Clause, "a breath at the dash");
    }

    #[test]
    fn semicolons_and_colons_pause_longer_than_commas() {
        let b = Book::new("t", "one, two; three: four. five");
        let p: Vec<Pause> = (0..4).map(|i| b.word(i).unwrap().pause).collect();
        assert_eq!(p, [Pause::Clause, Pause::Half, Pause::Half, Pause::Sentence]);
        let pace = Pacing::default();
        assert!(Pause::Clause.weight(&pace) < Pause::Half.weight(&pace) && Pause::Half.weight(&pace) < Pause::Sentence.weight(&pace));
    }

    #[test]
    fn splits_on_any_whitespace() {
        let b = Book::new("t", "  one\ttwo\r\nthree  ");
        assert_eq!(words(&b), ["one", "two", "three"]);
    }

    #[test]
    fn classifies_pauses() {
        let b = Book::new("t", "Hi, there. \"Quoted!\" plain end");
        let p: Vec<_> = (0..b.len()).map(|i| b.word(i).unwrap().pause).collect();
        assert_eq!(p, [Pause::Clause, Pause::Sentence, Pause::Sentence, Pause::Normal, Pause::Paragraph]);
    }

    #[test]
    fn blank_line_marks_paragraph() {
        let b = Book::new("t", "first para\n\nsecond");
        assert_eq!(b.word(1).unwrap().pause, Pause::Paragraph);
        let b = Book::new("t", "same\npara");
        assert_eq!(b.word(0).unwrap().pause, Pause::Normal);
    }

    #[test]
    fn empty_text_is_empty_book() {
        assert!(Book::new("t", " \n ").is_empty());
        assert_eq!(Book::new("t", "").weight_from(0), 0);
    }

    #[test]
    fn markup_is_stripped_and_kept_as_style() {
        let b = Book::new("t", "A **bold move**. Then *quiet _words_*, done.");
        assert_eq!(words(&b), ["A", "bold", "move.", "Then", "quiet", "words,", "done."]);
        let s = styles(&b);
        assert_eq!(s[0], (false, false, false));
        assert_eq!(s[1], (true, false, false));
        assert_eq!(s[2], (true, false, false));
        assert_eq!(s[3], (false, false, false));
        assert_eq!(s[4], (false, true, false));
        // `_words_*` closes the inner italic then the outer one; both toggles land.
        assert_eq!(s[6], (false, false, false));
    }

    #[test]
    fn headings_are_bold_double_and_end_a_paragraph() {
        let b = Book::new("t", "# Chapter One\nIt was dark.");
        assert_eq!(words(&b), ["Chapter", "One", "It", "was", "dark."]);
        let ch = b.word(0).unwrap();
        assert!(ch.style.heading() && ch.style.bold());
        assert_eq!(b.word(1).unwrap().pause, Pause::Paragraph);
        assert!(!b.word(2).unwrap().style.heading());
        let p = Pacing::default();
        let plain = Book::new("t", "Chapter word").word(0).unwrap();
        assert_eq!(ch.weight(&p), plain.weight(&p) * 2);
        let triple = Pacing { title: 30, ..p };
        assert_eq!(ch.weight(&triple), plain.weight(&p) * 3);
    }

    #[test]
    fn hash_without_text_or_inside_line_is_literal() {
        assert_eq!(words(&Book::new("t", "#\nuse #tags here")), ["#", "use", "#tags", "here"]);
        assert!(!Book::new("t", "#hashtag").word(0).unwrap().style.heading());
    }

    #[test]
    fn lone_bullets_are_dropped_without_toggling() {
        let b = Book::new("t", "* item one\n* item two");
        assert_eq!(words(&b), ["item", "one", "item", "two"]);
        assert!(styles(&b).iter().all(|s| !s.1));
    }

    #[test]
    fn chapter_lookup() {
        let b = Book::new("t", "Intro.\n# Part Two\nBody text.\n# Three\nEnd.");
        assert_eq!(b.chapter_at(0), None);
        assert_eq!(b.chapter_at(4), Some(1));
        assert_eq!(b.chapters().len(), 2);
        assert_eq!(b.chapters()[0], (1, "Part Two".into()));
        assert_eq!(b.chapters()[1].1, "Three");
        assert_eq!(b.chapter_index(6), Some(1));
    }

    #[test]
    fn groups_stop_at_punctuation_and_headings() {
        let b = Book::new("t", "# Title\nOne two, three four five. Six");
        assert_eq!(b.group(0, 3), 0..1, "heading never merges with body");
        assert_eq!(b.group(1, 3), 1..3, "stops after the comma");
        assert_eq!(b.group(3, 3), 3..6);
        assert_eq!(b.group(6, 3), 6..7);
        assert_eq!(b.group(1, 1), 1..2);
    }

    #[test]
    fn html_view_keeps_structure_and_escapes() {
        let b = Book::new("t", "# Title\nA <b> & **c**.\n\nNext");
        let mut s = String::new();
        b.write_html(&mut s).unwrap();
        assert!(s.starts_with("<h2><span id=\"w0\">Title</span></h2>"), "{s}");
        assert!(s.contains("&lt;b&gt;") && s.contains("&amp;"));
        assert!(s.contains("<span id=\"w4\"><b>c.</b></span></p>"), "{s}");
        assert!(s.contains("<p><span id=\"w5\">Next</span></p>"));
    }

    #[test]
    fn html_ranges_cover_whole_paragraphs() {
        let b = Book::new("t", "One two.\n\nThree four five.\n\nSix.");
        assert_eq!(b.paragraphs_around(3, 4), 2..5);
        assert_eq!(b.paragraphs_around(0, 1), 0..2);
        assert_eq!(b.paragraphs_around(4, 99), 2..6);
        let mut whole = String::new();
        b.write_html(&mut whole).unwrap();
        let mut parts = String::new();
        for r in [b.paragraphs_around(0, 1), b.paragraphs_around(2, 3), b.paragraphs_around(5, 6)] {
            b.write_html_range(r, &mut parts).unwrap();
        }
        assert_eq!(parts, whole, "pieces join into the same document");
    }

    fn image_bytes(images: &[(u16, u16)]) -> Vec<u8> {
        let mut b = IMAGES_MAGIC.to_vec();
        b.extend_from_slice(&(images.len() as u16).to_le_bytes());
        for &(w, h) in images {
            b.extend_from_slice(&w.to_le_bytes());
            b.extend_from_slice(&h.to_le_bytes());
            for k in 0..(w as u32 * h as u32) {
                b.extend_from_slice(&(k as u16).to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn images_are_words_of_their_own_with_pictures() {
        let mut file = b"Before the figure.\n\n{image:1}\n\n{image:0}\nAfter.".to_vec();
        file.extend(image_bytes(&[(2, 3), (4, 1)]));
        let b = Book::from_bytes(&file, "t");
        assert_eq!(b.len(), 6);
        let img = b.image_of(b.word(3).unwrap()).unwrap();
        assert_eq!((img.w, img.h, img.px.len()), (4, 1, 4), "{{image:1}} is the second picture");
        assert_eq!(b.image_of(b.word(4).unwrap()).unwrap().w, 2);
        let p = Pacing::default();
        assert_eq!(b.word(3).unwrap().weight(&p), 400, "an image stays 4× a plain word");
        assert_eq!(b.word(2).unwrap().pause, Pause::Paragraph, "the word before ends its paragraph");
        assert_eq!(b.group(2, 3), 2..3, "an image is never grouped with words");
        assert_eq!(b.phrase(3, 3), 3..4);
        let mut html = String::new();
        b.write_html(&mut html).unwrap();
        assert!(html.contains("<img src=\"/img/1\""), "{html}");
    }

    #[test]
    fn captions_belong_to_their_picture() {
        let b = Book::new("t", "Text before.\n\n{image:0}\n\n{caption} 20 Altamira (Spagna), *Bisonte* dipinto. {/caption}\n\nAfter.");
        assert_eq!(b.len(), 4, "the description isn't read as words");
        assert_eq!(b.caption(2), Some("20 Altamira (Spagna), *Bisonte* dipinto."));
        assert_eq!(b.caption(1), None);
        let mut html = String::new();
        b.write_html(&mut html).unwrap();
        assert!(html.contains("<figcaption>20 Altamira"), "{html}");
        let b = Book::new("t", "No picture {caption} stray {/caption} here");
        assert_eq!(b.len(), 3, "a stray caption is dropped, not read");
    }

    #[test]
    fn missing_or_broken_images_are_safe() {
        let b = Book::from_bytes(b"{image:5} text", "t");
        assert!(b.is_image(b.word(0).unwrap()) && b.image_of(b.word(0).unwrap()).is_none());
        let mut file = b"x".to_vec();
        file.extend_from_slice(IMAGES_MAGIC);
        file.extend_from_slice(&[1, 0, 200, 0, 200]); // truncated image
        assert!(Book::from_bytes(&file, "t").images.is_empty());
    }

    #[test]
    fn version_2_pictures_load_from_a_table() {
        // Two pictures, table first: 2×1 and 1×1.
        let mut sec = Vec::new();
        sec.extend_from_slice(&2u16.to_le_bytes());
        sec.extend_from_slice(&0u16.to_le_bytes());
        for (w, h, off) in [(2u16, 1u16, 20u32), (1, 1, 24)] {
            sec.extend_from_slice(&w.to_le_bytes());
            sec.extend_from_slice(&h.to_le_bytes());
            sec.extend_from_slice(&off.to_le_bytes());
        }
        sec.extend_from_slice(&[1, 0, 2, 0, 3, 0]);
        assert_eq!(image_table(&sec), [(2, 1, 20), (1, 1, 24)]);
        let mut file = b"A {image:1} B".to_vec();
        file.extend_from_slice(IMAGES_MAGIC2);
        file.extend_from_slice(&sec);
        let b = Book::from_bytes(&file, "t");
        assert_eq!(b.image_of(b.word(1).unwrap()).unwrap().px, [3]);
        assert_eq!(b.image_dims(b.word(1).unwrap()), Some((1, 1)));
        assert_eq!(split_images(&file).1, Some((13 + IMAGES_MAGIC2.len(), true)));
    }

    #[test]
    fn pictures_can_stay_in_storage() {
        struct Card;
        impl PictureSource for Card {
            fn len(&self) -> usize { 1 }
            fn dims(&self, n: usize) -> Option<(u16, u16)> { (n == 0).then_some((3, 2)) }
            fn load(&self, n: usize) -> Option<Image> { (n == 0).then(|| Image { w: 3, h: 2, px: alloc::vec![9; 6] }) }
        }
        let mut b = Book::new("t", "Look {image:0} here");
        b.images = Pictures::Stored(alloc::boxed::Box::new(Card));
        assert_eq!(b.image_dims(b.word(1).unwrap()), Some((3, 2)));
        assert_eq!(b.image_of(b.word(1).unwrap()).unwrap().px.len(), 6);
    }

    #[test]
    fn brackets_pause_like_commas() {
        let b = Book::new("t", "She said (quietly, almost to herself) nothing more");
        let pauses: Vec<Pause> = (0..b.len()).map(|i| b.word(i).unwrap().pause).collect();
        assert_eq!(pauses[1], Pause::Clause, "a breath before the aside opens");
        assert_eq!(pauses[5], Pause::Clause, "and as it closes");
        assert_eq!(pauses[6], Pause::Normal);
        let p = Pacing { long_words: false, ..Pacing::default() };
        assert_eq!(b.word(3).unwrap().weight(&p), 100, "asides go at the normal speed");
    }

    #[test]
    fn angle_brackets_and_guillemets_are_asides_too() {
        let b = Book::new("t", "He said «come here» and <<then>> left <x y> ok");
        let p: Vec<bool> = (0..b.len()).map(|i| b.word(i).unwrap().style.bracketed()).collect();
        assert_eq!(p, [false, false, true, true, false, true, false, true, true, false]);
        assert_eq!(bracket_pair(&b, 3), ("«", "»"));
        assert_eq!(bracket_pair(&b, 5), ("«", "»"), "<< >> shows as « »");
        assert_eq!(bracket_pair(&b, 8), ("<", ">"));
        assert_eq!(b.word(1).unwrap().pause, Pause::Clause, "a breath before the aside");
        let b = Book::new("t", "a (b [c] d) e");
        assert_eq!(bracket_pair(&b, 3), ("(", ")"));
        assert_eq!(bracket_pair(&b, 2), ("[", "]"), "the innermost pair shows");
        let b = Book::new("t", "see [the note here] and {this one} done");
        let p: Vec<bool> = (0..b.len()).map(|i| b.word(i).unwrap().style.bracketed()).collect();
        assert_eq!(p, [false, true, true, true, false, true, true, false]);
        assert_eq!(bracket_pair(&b, 6), ("{", "}"));
    }

    #[test]
    fn words_inside_parentheses_are_flagged() {
        let b = Book::new("t", "Before (inside one, inside (two) three) after (x).");
        let p: Vec<bool> = (0..b.len()).map(|i| b.word(i).unwrap().style.bracketed()).collect();
        assert_eq!(p, [false, true, true, true, true, true, false, true]);
        assert_eq!(words(&b)[1], "(inside", "the text itself is unchanged");
        let b = Book::new("t", "Open (never closed\n\nnext paragraph");
        assert!(!b.word(3).unwrap().style.bracketed(), "a paragraph ends a bracket that never closes");
        // Cancroregina, 53%: a page break splits the aside; it still holds.
        let b = Book::new("t", "passato. (Pomposo esordio, per codesto tuo\n\nfatterello. Eh, pensi ciò che vuole). Egualmente");
        let p: Vec<bool> = (0..b.len()).map(|i| b.word(i).unwrap().style.bracketed()).collect();
        assert_eq!(p, [false, true, true, true, true, true, true, true, true, true, true, true, false]);
        let b = Book::new("t", "Items: a) one, b) two (and more) three.");
        let p: Vec<bool> = (0..b.len()).map(|i| b.word(i).unwrap().style.bracketed()).collect();
        assert_eq!(p, [false, false, false, false, false, true, true, false], "list closers are not asides");
    }

    #[test]
    fn pause_marks_add_beats_and_are_not_words() {
        let p = Pacing::default();
        let b = Book::new("t", "Wait {pause} for it {pause:3} now");
        assert_eq!(words(&b), ["Wait", "for", "it", "now"]);
        assert_eq!(b.word(0).unwrap().weight(&p), 200, "one extra beat");
        assert_eq!(b.word(2).unwrap().weight(&p), 400, "three extra beats");
        assert_eq!(b.word(3).unwrap().style.beats(), 0);
    }

    #[test]
    fn slow_passages_stay_longer() {
        let p = Pacing::default();
        let b = Book::new("t", "fast {slow} slow words {/slow} fast");
        assert_eq!(words(&b), ["fast", "slow", "words", "fast"]);
        assert_eq!(b.word(1).unwrap().weight(&p), 140);
        assert_eq!(b.word(3).unwrap().weight(&p), Pause::Paragraph.weight(&p), "slow ended");
    }

    #[test]
    fn verse_lines_break_and_breathe() {
        let b = Book::new("t", "{verse}\nriverrun past Eve\nand Adam's from\n{/verse}\nprose line\nstill prose");
        let w = |i| b.word(i).unwrap();
        assert!(w(2).style.line_end() && w(2).pause >= Pause::Clause, "end of a verse line");
        assert!(w(5).style.line_end(), "end of the second verse line (from)");
        assert!(!w(7).style.line_end(), "prose lines don't break (line)");
        let mut html = String::new();
        b.write_html(&mut html).unwrap();
        assert!(html.contains("Eve</span><br>"), "{html}");
    }

    #[test]
    fn unknown_braces_are_ordinary_words() {
        assert_eq!(words(&Book::new("t", "a {curly} b {pause:x}")), ["a", "{curly}", "b", "{pause:x}"]);
    }

    #[test]
    fn orp_matches_table() {
        assert_eq!(split_orp("a"), ("", "a", ""));
        assert_eq!(split_orp("word"), ("w", "o", "rd"));
        assert_eq!(split_orp("reading"), ("re", "a", "ding"));
        assert_eq!(split_orp("\"Hello"), ("\"H", "e", "llo"));
        assert_eq!(split_orp("über"), ("ü", "b", "er"));
        assert_eq!(split_orp("..."), (".", ".", "."));
        // Letters count, not apostrophes or hyphens (Italian elisions).
        assert_eq!(split_orp("c'è").1, "è");
        assert_eq!(split_orp("un'opera").1, "o");
        assert_eq!(split_orp("l'arte").1, "a");
        assert_eq!(split_orp("dell'arte").1, "l");
        assert_eq!(split_orp("«Così»").1, "o");
        assert_eq!(split_orp("anti-aging").1, "t", "9 letters → the 3rd letter; the hyphen is not one");
    }

    #[test]
    fn long_words_split_at_a_syllable() {
        assert_eq!(split_long("contemporaneamente"), Some(("contempo-".into(), "raneamente".into())));
        assert_eq!(split_long("rappresentazione"), Some(("rappre-".into(), "sentazione".into())));
        assert_eq!(split_long("socio-economically"), Some(("socio-".into(), "economically".into())), "at its own hyphen");
        assert_eq!(split_long("antichissimi"), None, "13 letters or fewer stay whole");
        let (a, b) = split_long("precipitevolissimevolmente,").unwrap();
        assert!(a.ends_with('-') && b.ends_with(','), "{a} {b}");
    }

    #[test]
    fn lists_bulleted_and_numbered() {
        let b = Book::new("t", "Before the list.\n• apples and pears\n• bread\n\n• milk\nAfter it.\n\n1. first step\n2) second step\niv. roman\n- dialogue, not an item\n– nor this");
        assert_eq!(words(&b)[..6], ["Before", "the", "list.", "apples", "and", "pears"], "markers are not read");
        assert_eq!(b.lists.len(), 2);
        let (l, item) = b.list_at(5).unwrap();
        assert_eq!((l.items.len(), item, l.numbered()), (3, 0, false), "a blank line doesn't end a list");
        assert_eq!(b.list_at(6).unwrap().1, 1);
        assert!(b.list_at(8).is_none(), "\"After it.\" is past the list");
        let (n, _) = b.list_at(12).unwrap();
        assert!(n.numbered());
        assert_eq!(n.items.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>(), ["1.", "2)", "iv."]);
        assert!(b.list_at(b.len() - 1).is_none(), "dashes start dialogue, not lists");
        let nested = Book::new("t", "• fruit\n  • apples\n  • pears\n    • conference\n• bread\n\nSteps:\n1. first\n    a) inside\n2. second");
        assert_eq!(nested.lists.len(), 2);
        assert_eq!(nested.lists[0].levels, [0, 1, 1, 2, 0], "two spaces a level");
        assert_eq!(nested.lists[1].levels, [0, 1, 0], "four spaces work too");
        assert_eq!(nested.lists[0].depth(), 2);
        let lone = Book::new("t", "Index:\n\n1.\n\n2.\n\nThe end.");
        assert_eq!(words(&lone), ["Index:", "The", "end."], "lone numbers are labels, not words");
        // A catalogue: each picture comes before its numbered caption.
        let c = Book::new("t", "Works\n\n{image:0}\n\n1. Luigi, 2026.\n\n{image:1}\n\n2. DSG, 2025.\n\nEnd.");
        let pic = |n: usize| (0..c.len()).filter(|&i| c.is_image(c.word(i).unwrap())).nth(n).unwrap();
        assert_eq!(c.list_at(pic(0)).map(|(_, k)| k), Some(0), "picture 0 is item 1's");
        assert_eq!(c.list_at(pic(1)).map(|(_, k)| k), Some(1), "picture 1 is item 2's, not item 1's");
        // An item's text then its picture (EPUB lists): the picture stays with it.
        let e = Book::new("t", "• one\n\n{image:0}\n\n• two\n\n{image:1}\n\nEnd.");
        let epic = |n: usize| (0..e.len()).filter(|&i| e.is_image(e.word(i).unwrap())).nth(n).unwrap();
        assert_eq!(e.list_at(epic(0)).map(|(_, k)| k), Some(0), "after item one: item one's");
        let p = Book::new("t", "• one\n\n{image:0}\n{caption} A cat. {/caption}\n\n• two\nEnd.");
        assert_eq!(p.lists.len(), 1, "a picture inside a list keeps it one list");
        assert_eq!(p.lists[0].items.len(), 2);
        // A breath between items.
        assert!(b.word(5).unwrap().pause >= Pause::Sentence);
    }

    #[test]
    fn lists_and_tables_in_the_web_view() {
        let b = Book::new("t", "Intro.\n• one\n• two\n\n| A | B |\n| 1 | 2 |");
        let mut html = String::new();
        b.write_html_range(0..b.len(), &mut html).unwrap();
        assert!(html.contains("<span class=\"mk\">• </span><span id=\"w1\">one"), "{html}");
        assert!(html.contains("<br>\n<span class=\"mk\">• </span><span id=\"w2\">two"), "{html}");
        assert!(html.contains("<span class=\"mk\">· </span><span id=\"w4\">B"), "{html}");
        assert!(html.contains("<p><span id=\"w5\">1"), "each row its own line: {html}");
    }

    #[test]
    fn the_lists_and_tables_tour_has_them_all() {
        let b = Book::parse(include_str!("../../../books/tour-lists-and-tables.md"), "f");
        assert_eq!(b.title, "Lists and tables");
        assert_eq!(b.lists.iter().map(|l| l.items.len()).collect::<Vec<_>>(), [5, 5, 3, 12, 8, 15], "short and long, bulleted, numbered, lettered, nested");
        assert_eq!(b.lists[4].depth(), 2, "the nested one goes two levels deep");
        assert_eq!(b.tables.iter().map(|t| (t.rows, t.cols, t.header)).collect::<Vec<_>>(), [(4, 3, 1), (9, 4, 1), (13, 3, 1)]);
    }

    #[test]
    fn tables_read_row_by_row() {
        let b = Book::new("t", "Some works.\n\n| Year | Work |\n|---|:--:|\n| 1503 | Mona Lisa |\n| 1498 | |\n\nAfter.");
        assert_eq!(words(&b), ["Some", "works.", "Year", "Work", "1503", "Mona", "Lisa", "1498", "After."]);
        let t = &b.tables[0];
        assert_eq!((t.rows, t.cols, t.header), (3, 2, 1));
        assert_eq!(b.cell_at(5).map(|(t, c)| (t.cells[c].1, t.cells[c].2)), Some((1, 1)), "Mona: row 1, column 1");
        assert!(b.starts_cell(5) && !b.starts_cell(6));
        assert!(b.cell_at(8).is_none(), "\"After.\" is outside");
        assert_eq!(b.cell_text(t, 3), "Mona Lisa");
        assert_eq!(b.cell_text(t, 5), "", "an empty cell");
        assert!(b.word(3).unwrap().pause >= Pause::Paragraph, "a row ends with a breath");
        assert!(b.word(2).unwrap().pause >= Pause::Sentence, "and a cell, before the next column");
    }

    #[test]
    fn figure_references_point_to_their_picture() {
        let b = Book::new("t", "Grow them in jars (see figure 2) or in bags, as in fig. 1.\n\n{image:0}\n\n*Figure* *1:* A bag of straw.\n\n{image:1}\n\nFigure 2. A jar.\n\nAlso figure 7, which has no caption.");
        let pic = |k: usize| (0..b.len()).filter(|&i| b.is_image(b.word(i).unwrap())).nth(k).unwrap();
        let at = |w: &str| (0..b.len()).find(|&i| b.text_of(b.word(i).unwrap()) == w).unwrap();
        assert_eq!(b.figure_ref(at("figure")), Some(pic(1)), "figure 2 is the second picture");
        assert_eq!(b.figure_ref(at("2)")), Some(pic(1)), "its number too");
        assert_eq!(b.figure_ref(at("fig.")), Some(pic(0)));
        assert_eq!(b.figure_ref(pic(0) + 1), None, "a caption isn't a reference");
        assert_eq!(b.text_of(b.word(pic(0) + 1).unwrap()), "Figure");
        assert_eq!(b.figure_ref(at("7,")), None, "no caption for figure 7: read as words");
        assert!(b.weight_of(at("2)")..at("2)") + 1) > b.weight_of(at("in")..at("in") + 1), "the number stays a little longer");
        let two = Book::new("t", "See figure 26 and figure 27.\n\n{image:0}\n\n{image:1}\n\nFigure 26: Direct.\n\nFigure 27: Indirect.");
        let pics: Vec<usize> = (0..two.len()).filter(|&i| two.is_image(two.word(i).unwrap())).collect();
        assert_eq!((two.figure_ref(1), two.figure_ref(4)), (Some(pics[0]), Some(pics[1])), "two pictures, two captions: in order");
        let e = Book::new("t", "As figure 5 shows.\n\n{image:0}\n{caption} Figure 5. A cat. {/caption}\n\nEnd.");
        let epic = (0..e.len()).find(|&i| e.is_image(e.word(i).unwrap())).unwrap();
        assert_eq!(e.figure_ref(1), Some(epic), "numbered by its description (EPUB)");
    }

    #[test]
    fn a_table_is_one_sentence() {
        let b = Book::new("t", "Before it.\n\n| A | B |\n| one two | three |\n\nAfter it.");
        let first = b.tables[0].cells[0].0 as usize;
        assert_eq!(b.sentence_start(first + 3), first, "from inside: back to the table's start");
        assert_eq!(b.next_sentence(first), Some(b.tables[0].end as usize), "forward: past the whole table");
        assert_eq!(b.sentence_start(b.tables[0].end as usize), b.tables[0].end as usize, "after it: its own sentence");
    }

    #[test]
    fn tables_read_at_half_speed() {
        // The same word, in a table cell and outside: twice the time inside.
        let b = Book::new("t", "word here\n\n| word | x |\n| y | z |\n\nEnd.");
        let w = |i: usize| b.weight_of(i..i + 1);
        let outside = w(0);
        let cell = w(2); // "word" in a cell, then a breath before the next column
        assert_eq!(cell, Pause::Sentence.weight(&b.pacing) * 2, "a cell's word: its time (with the column's breath) doubled");
        assert_eq!(outside * 2, Pause::Normal.weight(&b.pacing) * 2);
    }

    #[test]
    fn author_comment_after_the_title() {
        let b = Book::parse("<!-- title: Cancroregina -->\n<!-- author: Tommaso Landolfi -->\n# I\nTesto.", "file");
        assert_eq!((b.title.as_str(), b.author.as_str()), ("Cancroregina", "Tommaso Landolfi"));
        assert_eq!(words(&b), ["I", "Testo."], "the author line is not read as words");
        assert_eq!(Book::parse("<!-- title: T -->\nTesto.", "f").author, "");
    }

    #[test]
    fn title_comment_names_the_book() {
        let b = Book::parse("<!-- title: Cancroregina -->\n# I\nTesto.", "file");
        assert_eq!(b.title, "Cancroregina");
        assert_eq!(words(&b), ["I", "Testo."]);
        assert_eq!(Book::parse("Plain.", "file").title, "file");
    }

    fn phrases(b: &Book, n: u8) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let r = b.phrase(i, n);
            out.push(r.clone().map(|j| b.text_of(b.word(j).unwrap())).collect::<Vec<_>>().join(" "));
            i = r.end;
        }
        out
    }

    #[test]
    fn phrases_are_fixed_size_like_the_original() {
        let b = Book::new("t", "She walked into the old house of the family, and waited.");
        assert_eq!(phrases(&b, 3), ["She walked into", "the old house", "of the family,", "and waited."]);
        assert_eq!(phrases(&b, 2)[0], "She walked");
    }

    #[test]
    fn phrases_end_at_sentences_but_not_commas() {
        let b = Book::new("t", "Yes, of course. No.");
        assert_eq!(phrases(&b, 3), ["Yes, of course.", "No."]);
    }

    #[test]
    fn phrases_keep_titles_apart_from_text() {
        let b = Book::new("t", "# Uno\nIl libro comincia.");
        assert_eq!(phrases(&b, 3), ["Uno", "Il libro comincia."]);
    }

    #[test]
    fn paragraph_navigation() {
        let b = Book::new("t", "One two.\n\nThree four.\n\nFive.");
        assert_eq!(b.paragraph_start(3), 2);
        assert_eq!(b.next_paragraph(0), Some(2));
        assert_eq!(b.next_paragraph(2), Some(4));
        assert_eq!(b.next_paragraph(4), None);
    }

    #[test]
    fn sentence_navigation() {
        let b = Book::new("t", "One two. Three four five. Six.");
        assert_eq!(b.sentence_start(3), 2);
        assert_eq!(b.sentence_start(2), 2);
        assert_eq!(b.sentence_start(0), 0);
        assert_eq!(b.next_sentence(0), Some(2));
        assert_eq!(b.next_sentence(3), Some(5));
        assert_eq!(b.next_sentence(5), None);
    }

    #[test]
    fn weights_accumulate() {
        let b = Book::new("t", "a b.");
        let p = Pacing::default();
        assert_eq!(b.weight_from(0), 100 + Pause::Paragraph.weight(&p));
        assert_eq!(b.weight_from(1), Pause::Paragraph.weight(&p));
        assert_eq!(b.weight_from(99), 0);
    }

    #[test]
    fn long_words_linger_but_capped() {
        let p = Pacing::default();
        let short = Book::new("t", "cat").word(0).unwrap();
        let long = Book::new("t", "antidisestablishmentarianism").word(0).unwrap();
        assert!(long.weight(&p) > short.weight(&p));
        assert!(long.weight(&p) <= Pause::Paragraph.weight(&p) + 60);
        let off = Pacing { long_words: false, ..p };
        assert_eq!(long.weight(&off), short.weight(&off));
    }
}

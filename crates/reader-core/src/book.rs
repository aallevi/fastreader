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
    Clause,
    Sentence,
    Paragraph,
}

impl Pause {
    /// Display time as a percentage of the base word time.
    pub const fn weight(self, p: &Pacing) -> u32 {
        match self {
            Pause::Normal => 100,
            Pause::Clause => p.comma as u32 * 10,
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
        if self.style.heading() { w * p.title as u32 / 10 } else { w }
    }
}

pub struct Book {
    pub title: String,
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
        let (text, words) = tokenize(source);
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
        let mut b = Book { title: title.into(), text, words, cum: Vec::new(), chapters, pacing: Pacing::default() };
        b.set_pacing(&Pacing::default());
        b
    }

    /// Builds a book from converter output: an optional first line
    /// `<!-- title: ... -->` names it, otherwise `fallback` does.
    pub fn parse(source: &str, fallback: &str) -> Self {
        let (title, body) = match source.strip_prefix("<!-- title:").and_then(|r| r.split_once("-->")) {
            Some((t, rest)) => (t.trim(), rest),
            None => (fallback, source),
        };
        Book::new(title, body)
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
            acc += w.weight(p);
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
        self.cum[c] + self.words.range(c * CHECKPOINT..i).map(|w| w.weight(&self.pacing)).sum::<u32>()
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
        let mut j = i.min(self.words.len().saturating_sub(1));
        while j > 0 && self.words[j - 1].pause < Pause::Sentence {
            j -= 1;
        }
        j
    }

    /// First word of the sentence after the one containing `i`, if any.
    pub fn next_sentence(&self, i: usize) -> Option<usize> {
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

    /// Summed weight of a range of words.
    pub fn weight_of(&self, r: core::ops::Range<usize>) -> u32 {
        let e = r.end.min(self.words.len());
        let s = r.start.min(e);
        self.words.range(s..e).map(|w| w.weight(&self.pacing)).sum()
    }

    /// Writes the book as HTML for the phone/computer web view. Every word is
    /// a `<span id="wN">` so the page can highlight the word being read.
    /// Streams into any `fmt::Write`, so the ESP32 can send it in chunks.
    pub fn write_html(&self, out: &mut impl Write) -> fmt::Result {
        let mut open = false;
        for (i, w) in self.words.iter().enumerate() {
            if !open {
                out.write_str(if w.style.heading() { "<h2>" } else { "<p>" })?;
                open = true;
            }
            let (b, it) = (w.style.bold() && !w.style.heading(), w.style.italic());
            write!(out, "<span id=\"w{i}\">")?;
            if b { out.write_str("<b>")?; }
            if it { out.write_str("<i>")?; }
            escape_html(self.text_of(*w), out)?;
            if it { out.write_str("</i>")?; }
            if b { out.write_str("</b>")?; }
            out.write_str("</span>")?;
            let heading_ends = w.style.heading()
                && self.words.get(i + 1).is_none_or(|n| !n.style.heading() || w.pause == Pause::Paragraph);
            if w.pause == Pause::Paragraph || heading_ends {
                out.write_str(if w.style.heading() { "</h2>\n" } else { "</p>\n" })?;
                open = false;
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

fn tokenize(source: &str) -> (ChunkedText, Chunked<Word>) {
    let mut text = ChunkedText::default();
    let mut words: Chunked<Word> = Chunked::new();
    let mut style = Style::PLAIN;
    let mut scratch = String::new();

    for line in source.lines() {
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
        let first_word = words.len();

        for tok in tokens {
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
            words.push(Word {
                start: text.push(shown),
                len: shown.len() as u16,
                pause: classify(shown),
                style: style.with(Style::HEADING, heading),
            });
            for m in trail_markers {
                style = toggle(style, m);
            }
        }
        if heading && words.len() > first_word {
            words.last_mut().unwrap().pause = Pause::Paragraph;
        }
    }
    if let Some(last) = words.last_mut() {
        last.pause = Pause::Paragraph;
    }
    text.shrink_to_fit();
    words.shrink_to_fit();
    (text, words)
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

fn classify(word: &str) -> Pause {
    // Look past closing quotes/brackets: `end."` still ends a sentence.
    let tail = word.trim_end_matches(['"', '\'', ')', ']', '»', '”', '’']);
    match tail.chars().last() {
        Some('.' | '!' | '?' | '…') => Pause::Sentence,
        Some(',' | ';' | ':' | '—' | '–') => Pause::Clause,
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
    let total = word.chars().count();
    (lead + idx).min(total.saturating_sub(1))
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
    fn orp_matches_table() {
        assert_eq!(split_orp("a"), ("", "a", ""));
        assert_eq!(split_orp("word"), ("w", "o", "rd"));
        assert_eq!(split_orp("reading"), ("re", "a", "ding"));
        assert_eq!(split_orp("\"Hello"), ("\"H", "e", "llo"));
        assert_eq!(split_orp("über"), ("ü", "b", "er"));
        assert_eq!(split_orp("..."), (".", ".", "."));
    }

    #[test]
    fn title_comment_names_the_book() {
        let b = Book::parse("<!-- title: Cancroregina -->\n# I\nTesto.", "file");
        assert_eq!(b.title, "Cancroregina");
        assert_eq!(words(&b), ["I", "Testo."]);
        assert_eq!(Book::parse("Plain.", "file").title, "file");
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

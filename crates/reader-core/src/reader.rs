//! Playback: which word is showing and when the next one is due.
//!
//! Time is passed in as milliseconds from any monotonic clock, so the same
//! code runs on the ESP32 timer, the desktop clock and in tests.

use crate::book::Book;
use crate::settings::{Pacing, Skip};

pub const MIN_WPM: u16 = 100;
pub const MAX_WPM: u16 = 1000;
pub const WPM_STEP: u16 = 25;

/// Words shown at reduced speed after (re)starting, so the eye can lock on.
const RAMP_WORDS: usize = 4;

/// A picture's time starts when it is shown; if the platform doesn't say
/// so, at most this much later.
pub const PICTURE_GRACE_MS: u64 = 3_000;

/// A second ◀ within this long of the first goes one unit further back.
pub const BACK_AGAIN_MS: u64 = 2_500;

pub struct Reader {
    pub book: Book,
    pos: usize,
    wpm: u16,
    playing: bool,
    next_due: u64,
    ramp: usize,
    /// Words per flash (1–3) in Word mode.
    chunk: u8,
    /// Phrase grouping (Phrase/Page modes): words per group, or 0 for off.
    phrase: u8,
    /// Reached the end by playing; the next play starts over.
    finished: bool,
    /// Long words in two flashes (Word mode with the recognition point).
    split: bool,
    /// Showing the second half of a split word.
    second: bool,
    /// A picture came up and isn't confirmed on screen yet: its time starts
    /// when the platform says the frame is shown (`frame_shown`).
    picture_pending: bool,
    /// The last backward jump: (when, where it landed). A press soon after
    /// counts from there, so pressing ◀ twice goes two units back even while
    /// playing moved on a word or two (as a music player's ⏮).
    last_back: Option<(u64, usize)>,
    pacing: Pacing,
    /// The wpm number is the real average pace, pauses included.
    true_pace: bool,
}

impl Reader {
    pub fn new(book: Book, wpm: u16) -> Self {
        Reader { book, pos: 0, wpm: wpm.clamp(MIN_WPM, MAX_WPM), playing: false, next_due: 0, ramp: 0, chunk: 1, phrase: 0, finished: false, split: false, second: false, picture_pending: false, last_back: None, pacing: Pacing::default(), true_pace: false }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn wpm(&self) -> u16 {
        self.wpm
    }
    pub fn is_playing(&self) -> bool {
        self.playing
    }
    pub fn at_end(&self) -> bool {
        self.shown().end >= self.book.len()
    }

    /// Swaps in another book, keeping speed and settings; stops playback.
    pub fn set_book(&mut self, mut book: Book, pos: usize) {
        book.set_pacing(&self.pacing);
        self.book = book;
        self.playing = false;
        self.finished = false;
        self.pos = pos.min(self.book.len().saturating_sub(1));
    }

    pub fn set_pacing(&mut self, p: Pacing) {
        if p != self.pacing {
            self.pacing = p;
            self.book.set_pacing(&p);
        }
    }

    pub fn chunk(&self) -> u8 {
        self.chunk
    }

    pub fn set_chunk(&mut self, n: u8) {
        self.chunk = n.clamp(1, 3);
    }

    /// Phrase grouping with `Some(words)` (ends early at sentences), or
    /// Word-mode grouping with `None` (ends early at any punctuation).
    pub fn set_phrase(&mut self, words: Option<u8>) {
        self.phrase = words.map_or(0, |n| n.clamp(1, 3));
    }

    pub fn phrase_mode(&self) -> bool {
        self.phrase > 0
    }

    /// The words on screen now (one, a few, or a phrase).
    pub fn shown(&self) -> core::ops::Range<usize> {
        if self.phrase > 0 {
            self.book.phrase(self.pos, self.phrase)
        } else {
            self.book.group(self.pos, self.chunk)
        }
    }

    /// How far through the current group's display time we are (0.0–1.0);
    /// the ticker uses it to glide smoothly between words.
    pub fn progress(&self, now: u64) -> f32 {
        if !self.playing {
            return 0.0;
        }
        let d = self.current_delay().max(1);
        let start = self.next_due.saturating_sub(d);
        (now.saturating_sub(start) as f32 / d as f32).clamp(0.0, 1.0)
    }

    pub fn set_split(&mut self, on: bool) {
        self.split = on;
        self.second = false;
    }

    /// For a long word shown in two flashes: `Some(false)` for its first
    /// half, `Some(true)` for the second. `None` otherwise.
    pub fn split_part(&self) -> Option<bool> {
        if !self.split {
            return None;
        }
        let s = self.shown();
        let w = self.book.word(s.start)?;
        (s.len() == 1 && !self.book.is_image(w) && crate::book::split_long(self.book.text_of(w)).is_some()).then_some(self.second)
    }

    /// A picture on screen right now (shown alone).
    pub fn on_picture(&self) -> bool {
        let s = self.shown();
        s.len() == 1 && self.book.word(s.start).is_some_and(|w| self.book.is_image(w))
    }

    /// Paused by itself on a picture ("Until I press play").
    pub fn held_on_picture(&self) -> bool {
        !self.playing && !self.finished && self.on_picture() && crate::settings::PICTURE_MS[self.pacing.picture.min(4) as usize] == 0
    }

    /// Display time of the current word in ms, including the start-up ramp.
    pub fn current_delay(&self) -> u64 {
        if self.book.is_empty() {
            return 0;
        }
        if self.on_picture() {
            // Pictures stay a set time, whatever the reading speed; with
            // "until I press play" the press itself moves on (delay 0).
            return crate::settings::PICTURE_MS[self.pacing.picture.min(4) as usize];
        }
        let base = self.ms_for(self.book.weight_of(self.shown()) as u64);
        // Each half of a split word gets half its time.
        let base = if self.split_part().is_some() { base / 2 } else { base };
        if self.ramp > 0 && self.pacing.ramp {
            base * (100 + 25 * self.ramp as u64) / 100
        } else {
            base
        }
    }

    pub fn ms_left(&self) -> u64 {
        self.ms_for(self.book.weight_from(self.pos) as u64)
    }

    /// Make the wpm number mean the real average pace (pauses included)
    /// instead of the base rate for a plain word.
    pub fn set_true_pace(&mut self, on: bool) {
        self.true_pace = on;
    }

    /// Milliseconds for `weight` (in % of a plain word). One "word" of the
    /// wpm number is a plain word (100), or with real pace the book's
    /// average word including its pauses (exact fraction, no rounding).
    fn ms_for(&self, weight: u64) -> u64 {
        let total = self.book.weight_from(0) as u64;
        let n = self.book.len() as u64;
        if self.true_pace && n > 0 && total > n * 100 {
            60_000 * weight * n / (self.wpm as u64 * total)
        } else {
            60_000 * weight / (self.wpm as u64 * 100)
        }
    }

    /// Progress through the book in 1/1000ths.
    pub fn permille(&self) -> u16 {
        match self.book.len() {
            0 | 1 => 1000 * self.pos.min(1) as u16,
            n => (self.pos as u64 * 1000 / (n as u64 - 1)) as u16,
        }
    }

    pub fn play(&mut self, now: u64) {
        if self.book.is_empty() {
            return;
        }
        if self.finished {
            self.pos = 0;
            self.finished = false;
        }
        self.playing = true;
        self.ramp = RAMP_WORDS;
        self.next_due = now + self.current_delay();
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Changes speed; returns false if already at the limit.
    pub fn nudge_wpm(&mut self, up: bool) -> bool {
        let new = if up { self.wpm.saturating_add(WPM_STEP) } else { self.wpm.saturating_sub(WPM_STEP) };
        let new = new.clamp(MIN_WPM, MAX_WPM);
        let changed = new != self.wpm;
        self.wpm = new;
        changed
    }

    pub fn set_wpm(&mut self, wpm: u16) {
        self.wpm = wpm.clamp(MIN_WPM, MAX_WPM);
    }

    /// Jumps to a word; playback restarts its ramp from there.
    pub fn seek(&mut self, pos: usize, now: u64) {
        self.pos = pos.min(self.book.len().saturating_sub(1));
        self.second = false;
        self.picture_pending = false;
        self.finished = false;
        if self.playing {
            self.ramp = RAMP_WORDS;
            self.next_due = now + self.current_delay();
        }
    }

    /// Back to the start of this sentence, or the previous one if already
    /// there. Returns false at the very beginning.
    pub fn sentence_back(&mut self, now: u64) -> bool {
        self.resume_last_back(now);
        let here = self.book.sentence_start(self.pos);
        let target = if here == self.pos && here > 0 { self.book.sentence_start(here - 1) } else { here };
        let moved = target != self.pos;
        self.seek(target, now);
        self.last_back = Some((now, target));
        moved
    }

    /// Within [`BACK_AGAIN_MS`] of the last backward jump, and not moved
    /// back past it by hand: count from where that jump landed.
    fn resume_last_back(&mut self, now: u64) {
        if let Some((t, at)) = self.last_back {
            if now.saturating_sub(t) < BACK_AGAIN_MS && at <= self.pos && self.book.sentence_start(self.pos) <= at {
                self.pos = at;
            }
        }
    }

    /// The group starting at `i`, by this reader's grouping.
    fn group_at(&self, i: usize) -> core::ops::Range<usize> {
        if self.phrase > 0 { self.book.phrase(i, self.phrase) } else { self.book.group(i, self.chunk) }
    }

    /// One phrase (group) back or forward; groups are the same ones the
    /// reader flashes, so stepping back and forth lands on the same groups.
    pub fn step_phrase(&mut self, forward: bool, now: u64) -> bool {
        let n = self.book.len();
        let target = if forward {
            let e = self.shown().end;
            (e < n).then_some(e)
        } else {
            self.resume_last_back(now);
            if self.pos == 0 {
                return false;
            }
            // Walk the groups from the start of the sentence before us.
            let mut s = self.book.sentence_start(self.pos - 1);
            loop {
                let e = self.group_at(s).end;
                if e >= self.pos || e <= s {
                    break Some(s);
                }
                s = e;
            }
        };
        match target {
            Some(t) => {
                self.seek(t, now);
                if !forward {
                    self.last_back = Some((now, t));
                }
                true
            }
            None => false,
        }
    }

    /// One word back or forward. Returns false at either end.
    pub fn step_word(&mut self, forward: bool, now: u64) -> bool {
        let target = if forward { self.pos + 1 } else { self.pos.wrapping_sub(1) };
        if target >= self.book.len() {
            return false;
        }
        self.seek(target, now);
        true
    }

    /// ◀◀ by the chosen amount. Returns false at the very beginning.
    pub fn skip_back(&mut self, skip: Skip, now: u64) -> bool {
        if skip == Skip::Paragraph {
            if let Some((t, at)) = self.last_back {
                if now.saturating_sub(t) < BACK_AGAIN_MS && at <= self.pos && self.book.paragraph_start(self.pos) <= at {
                    self.pos = at;
                }
            }
        }
        let target = match skip {
            Skip::Sentence => return self.sentence_back(now),
            Skip::Words(n) => self.pos.saturating_sub(n as usize),
            Skip::Paragraph => {
                let here = self.book.paragraph_start(self.pos);
                if here == self.pos && here > 0 { self.book.paragraph_start(here - 1) } else { here }
            }
        };
        let moved = target != self.pos;
        self.seek(target, now);
        self.last_back = Some((now, target));
        moved
    }

    /// ▶▶ by the chosen amount. Returns false at the end.
    pub fn skip_forward(&mut self, skip: Skip, now: u64) -> bool {
        let target = match skip {
            Skip::Sentence => return self.sentence_forward(now),
            Skip::Words(n) => Some(self.pos + n as usize).filter(|&t| t < self.book.len()),
            Skip::Paragraph => self.book.next_paragraph(self.pos),
        };
        match target {
            Some(t) => {
                self.seek(t, now);
                true
            }
            None => false,
        }
    }

    pub fn sentence_forward(&mut self, now: u64) -> bool {
        match self.book.next_sentence(self.pos) {
            Some(n) => {
                self.seek(n, now);
                true
            }
            None => false,
        }
    }

    /// Advances as many words as are due. Returns true if the word changed.
    /// Late ticks never cause a burst: if we fell more than one word behind,
    /// the schedule resets from `now`.
    pub fn tick(&mut self, now: u64) -> bool {
        if !self.playing || now < self.next_due {
            return false;
        }
        if self.at_end() {
            self.playing = false;
            self.finished = true;
            return false;
        }
        if self.split_part() == Some(false) {
            self.second = true;
            let delay = self.current_delay();
            self.next_due = if now - self.next_due > delay { now + delay } else { self.next_due + delay };
            return true;
        }
        self.second = false;
        self.pos = self.shown().end;
        self.ramp = self.ramp.saturating_sub(1);
        if self.on_picture() && crate::settings::PICTURE_MS[self.pacing.picture.min(4) as usize] == 0 {
            self.playing = false; // wait on the picture until ▶
            return true;
        }
        if self.on_picture() {
            // Loading a picture (from the SD card) and drawing it takes a
            // moment: its time counts from when it is on screen. If the
            // platform never says, it starts anyway a few seconds later.
            self.picture_pending = true;
            self.next_due = now + PICTURE_GRACE_MS + self.current_delay();
            return true;
        }
        let delay = self.current_delay();
        self.next_due = if now - self.next_due > delay { now + delay } else { self.next_due + delay };
        true
    }

    /// The platform has put the current frame on the screen: a picture that
    /// just came up starts its time now.
    pub fn frame_shown(&mut self, now: u64) {
        if self.picture_pending {
            self.picture_pending = false;
            if self.playing && self.on_picture() {
                self.next_due = now + self.current_delay();
            }
        }
    }

    /// When the platform should wake up next, if playing.
    pub fn next_due(&self) -> Option<u64> {
        self.playing.then_some(self.next_due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use core::fmt::Write;

    fn plain_book(n: usize) -> Book {
        let mut s = String::new();
        for i in 0..n {
            write!(s, "w{} ", i % 10).unwrap();
        }
        Book::new("t", &s)
    }

    fn run_to_end(r: &mut Reader, step: u64) -> u64 {
        let mut now = 0;
        r.play(now);
        while r.is_playing() {
            now += step;
            r.tick(now);
        }
        now
    }

    #[test]
    fn plain_text_hits_target_wpm() {
        // 1000 plain words at 500 wpm ≈ 2 minutes (+ ramp + final paragraph).
        let mut r = Reader::new(plain_book(1000), 500);
        let total = run_to_end(&mut r, 1);
        let ideal = 120_000;
        assert!(total >= ideal && total < ideal + 1_500, "took {total} ms");
        assert_eq!(r.pos(), 999);
    }

    #[test]
    fn coarse_ticks_do_not_drift() {
        // A 10 ms scheduler must land on the same total as 1 ms ticks (±1 tick per word is not allowed to accumulate).
        let mut a = Reader::new(plain_book(500), 300);
        let mut b = Reader::new(plain_book(500), 300);
        let ta = run_to_end(&mut a, 1);
        let tb = run_to_end(&mut b, 10);
        assert!(tb.abs_diff(ta) <= 10, "{ta} vs {tb}");
    }

    #[test]
    fn late_wakeup_skips_no_words() {
        let mut r = Reader::new(plain_book(100), 600);
        r.play(0);
        assert!(r.tick(10_000));
        assert_eq!(r.pos(), 1, "one tick advances exactly one word");
    }

    #[test]
    fn wpm_limits_report_no_change() {
        let mut r = Reader::new(plain_book(3), MAX_WPM);
        assert!(!r.nudge_wpm(true));
        r.set_wpm(MIN_WPM);
        assert!(!r.nudge_wpm(false));
        assert!(r.nudge_wpm(true));
        assert_eq!(r.wpm(), MIN_WPM + WPM_STEP);
    }

    #[test]
    fn sentence_back_twice_goes_further() {
        let mut r = Reader::new(Book::new("t", "A b. C d e. F g."), 300);
        r.seek(3, 0);
        assert!(r.sentence_back(0));
        assert_eq!(r.pos(), 2);
        assert!(r.sentence_back(0));
        assert_eq!(r.pos(), 0);
        assert!(!r.sentence_back(0));
    }

    #[test]
    fn a_picture_counts_from_when_it_is_on_screen() {
        let mut r = Reader::new(Book::new("t", "One.\n\n{image:0}\n\nTwo."), 475);
        r.set_pacing(Pacing { ramp: false, picture: 0, ..Pacing::default() }); // 2 s
        r.play(0);
        let mut t = 0;
        while r.pos() == 0 {
            t += 5;
            r.tick(t);
        }
        assert!(r.on_picture());
        // Loading from the card took 900 ms; then the frame is shown.
        r.frame_shown(t + 900);
        r.tick(t + 2_800);
        assert_eq!(r.pos(), 1, "still on the picture: only 1.9 s on screen");
        r.tick(t + 2_905);
        assert_eq!(r.pos(), 2, "2 s after it appeared, reading moves on");
    }

    #[test]
    fn long_words_take_two_flashes_in_the_same_time() {
        let mut r = Reader::new(Book::new("t", "Era contemporaneamente qui."), 300);
        r.set_pacing(Pacing { ramp: false, long_words: false, ..Pacing::default() });
        r.seek(1, 0);
        let whole = r.current_delay();
        r.set_split(true);
        assert_eq!(r.split_part(), Some(false));
        assert_eq!(r.current_delay(), whole / 2);
        r.seek(0, 0);
        r.play(0);
        let mut seen = alloc::vec::Vec::new();
        for t in (0..3000).step_by(5) {
            if r.tick(t) {
                seen.push((r.pos(), r.split_part()));
            }
        }
        assert_eq!(seen[..3], [(1, Some(false)), (1, Some(true)), (2, None)]);
    }

    #[test]
    fn pictures_stay_their_set_time_or_wait() {
        let mut r = Reader::new(Book::new("t", "One two.\n\n{image:0}\n\nThree four."), 475);
        r.set_pacing(Pacing { ramp: false, picture: 2, ..Pacing::default() });
        r.seek(2, 0);
        assert!(r.on_picture());
        assert_eq!(r.current_delay(), 8_000, "8 seconds, whatever the speed");
        r.set_wpm(900);
        assert_eq!(r.current_delay(), 8_000);
        r.set_pacing(Pacing { ramp: false, picture: 4, ..Pacing::default() });
        r.seek(0, 0);
        r.play(0);
        let mut t = 0;
        while r.is_playing() && t < 20_000 {
            t += 10;
            r.tick(t);
        }
        assert_eq!(r.pos(), 2, "stopped on the picture");
        assert!(r.held_on_picture());
        r.play(t);
        r.tick(t + 1);
        assert_eq!(r.pos(), 3, "play moves on at once");
    }

    #[test]
    fn pressing_back_again_soon_goes_further_even_while_playing() {
        let mut r = Reader::new(Book::new("t", "One two three. Four five six. Seven eight nine ten eleven twelve."), 300);
        r.set_pacing(Pacing { ramp: false, ..Pacing::default() });
        r.seek(9, 0);
        r.play(0);
        assert!(r.sentence_back(1000));
        assert_eq!(r.pos(), 6);
        for t in (1000..1900).step_by(10) {
            r.tick(t);
        }
        assert!(r.pos() > 6, "playing moved on");
        assert!(r.sentence_back(1900));
        assert_eq!(r.pos(), 3, "second press within the window: the sentence before");
        r.tick(5000);
        let at = r.pos();
        assert!(r.sentence_back(9000));
        assert_eq!(r.pos(), 3.min(at), "later on, a press restarts the current sentence");
    }

    #[test]
    fn phrase_back_twice_while_playing() {
        let mut r = Reader::new(Book::new("t", "She walked into the old house of the family, and waited. Then left."), 300);
        r.set_phrase(Some(3));
        r.set_pacing(Pacing { ramp: false, ..Pacing::default() });
        r.seek(6, 0);
        assert!(r.step_phrase(false, 100));
        assert_eq!(r.pos(), 3);
        r.seek(4, 600); // as if playing moved on a word
        assert!(r.step_phrase(false, 700));
        assert_eq!(r.pos(), 0, "counted from the last jump");
    }

    #[test]
    fn play_restarts_only_after_finishing() {
        let mut r = Reader::new(plain_book(6), 300);
        r.set_chunk(2);
        r.seek(4, 0);
        r.play(0);
        assert_eq!(r.pos(), 4, "the last group is still shown, not skipped");
        run_to_end(&mut r, 5);
        r.play(0);
        assert_eq!(r.pos(), 0, "after finishing, play starts over");
    }

    #[test]
    fn chunks_keep_the_same_reading_speed() {
        let mut one = Reader::new(plain_book(600), 400);
        let mut three = Reader::new(plain_book(600), 400);
        three.set_chunk(3);
        let (a, b) = (run_to_end(&mut one, 1), run_to_end(&mut three, 1));
        assert!(a.abs_diff(b) < 1_500, "1 word: {a} ms, 3 words: {b} ms");
        assert_eq!(three.pos(), 597, "ends on the last group");
    }

    #[test]
    fn skip_by_words_and_paragraphs() {
        let mut r = Reader::new(Book::new("t", "a b c d e f g h i j k l.\n\nm n o.\n\np q."), 300);
        assert!(r.skip_forward(Skip::Words(5), 0));
        assert_eq!(r.pos(), 5);
        assert!(r.skip_back(Skip::Words(10), 0));
        assert_eq!(r.pos(), 0);
        assert!(!r.skip_back(Skip::Words(10), 0));
        assert!(r.skip_forward(Skip::Paragraph, 0));
        assert_eq!(r.pos(), 12);
        r.seek(14, 0);
        assert!(r.skip_back(Skip::Paragraph, 0));
        assert_eq!(r.pos(), 12, "first to the start of this paragraph");
        assert!(r.skip_back(Skip::Paragraph, 0));
        assert_eq!(r.pos(), 0, "then to the previous one");
        assert!(!r.skip_forward(Skip::Words(50), 0));
    }

    #[test]
    fn true_pace_makes_the_number_the_real_speed() {
        let text = "One, two. Three four, five six. Seven eight nine, ten.\n\nEleven twelve.";
        let mut book = String::new();
        for _ in 0..100 {
            book.push_str(text);
            book.push_str("\n\n");
        }
        let words = Book::new("t", &book).len() as u64;
        let mut r = Reader::new(Book::new("t", &book), 300);
        r.set_pacing(Pacing { ramp: false, ..Pacing::default() });
        let base = r.ms_left();
        r.set_true_pace(true);
        let real = r.ms_left();
        assert!(base > words * 200, "with pauses, base 300 wpm takes longer than 300 words a minute");
        assert!(real.abs_diff(words * 200) <= words / 2, "true pace: {real} ms for {words} words at 300 wpm");
    }

    #[test]
    fn phrase_steps_go_group_by_group_both_ways() {
        let mut r = Reader::new(Book::new("t", "She walked into the old house of the family, and waited. Then left."), 300);
        r.set_phrase(Some(3));
        let mut starts = alloc::vec![0];
        while r.step_phrase(true, 0) {
            starts.push(r.pos());
        }
        assert_eq!(starts, [0, 3, 6, 9, 11], "She walked into | the old house | of the family, | and waited. | Then left.");
        for &s in starts.iter().rev().skip(1) {
            assert!(r.step_phrase(false, 0));
            assert_eq!(r.pos(), s);
        }
        assert!(!r.step_phrase(false, 0), "nothing before the first group");
    }

    #[test]
    fn ramp_can_be_turned_off() {
        let mut r = Reader::new(plain_book(10), 600);
        r.play(0);
        let with = r.current_delay();
        r.set_pacing(Pacing { ramp: false, ..Pacing::default() });
        assert!(r.current_delay() < with);
        assert_eq!(r.current_delay(), 100);
    }

    #[test]
    fn phrase_grouping_advances_by_whole_phrases() {
        let mut r = Reader::new(Book::new("t", "She walked into the old house of the family, and waited."), 300);
        r.set_phrase(Some(2));
        assert_eq!(r.shown(), 0..2);
        r.play(0);
        let mut now = 0;
        while r.pos() == 0 {
            now += 5;
            r.tick(now);
        }
        assert_eq!(r.pos(), 2, "jumps to the next phrase");
        r.set_phrase(None);
        assert_eq!(r.shown(), 2..3);
    }

    #[test]
    fn progress_runs_from_zero_to_one_per_word() {
        let mut r = Reader::new(plain_book(10), 600);
        r.set_pacing(Pacing { ramp: false, ..Pacing::default() });
        r.play(0);
        assert_eq!(r.progress(0), 0.0);
        assert!((r.progress(50) - 0.5).abs() < 0.01);
        r.pause();
        assert_eq!(r.progress(70), 0.0);
    }

    #[test]
    fn empty_book_never_plays() {
        let mut r = Reader::new(Book::new("t", ""), 300);
        r.play(0);
        assert!(!r.is_playing());
        assert_eq!(r.permille(), 0);
        assert_eq!(r.ms_left(), 0);
    }
}

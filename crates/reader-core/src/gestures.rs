//! Turns raw button/touch state into app events with one set of timing rules.
//!
//! Touch works like a real button on any desktop or phone: the target lights
//! when the finger lands, acts when it lifts, and sliding off cancels. There
//! are no swipes and no invisible zones: only drawn buttons and list rows.
//! Holding a direction button repeats it (and ◀◀/▶▶ opens the line view).

use crate::app::Hit;
use crate::input::Key;

pub const HOLD_MS: u64 = 400;
pub const REPEAT_MS: u64 = 110;
pub const LONG_MS: u64 = 700;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Finger landed on a target (or empty space): light it.
    Down(Option<Hit>),
    /// Finger lifted on the target it landed on.
    Tap(Hit),
    /// Still holding a repeating button.
    Held(Key),
    /// A hold ended.
    Release,
    /// Finger slid off its target: unlight, do nothing.
    Cancel,
    /// A physical key press.
    Press(Key),
}

#[derive(Default)]
pub struct Touch {
    start: Option<(Option<Hit>, u64)>,
    last_repeat: u64,
    repeating: bool,
    cancelled: bool,
}

impl Touch {
    /// Feed every poll: `None` = no finger, `Some(target)` = finger down over
    /// `target` (which is `None` over empty space).
    pub fn update(&mut self, finger: Option<Option<Hit>>, now: u64) -> Option<Event> {
        match (finger, self.start) {
            (Some(target), None) => {
                self.start = Some((target, now));
                self.repeating = false;
                self.cancelled = false;
                Some(Event::Down(target))
            }
            (Some(target), Some((first, t0))) => {
                if self.cancelled {
                    return None;
                }
                if target != first && !self.repeating {
                    self.cancelled = true;
                    return Some(Event::Cancel);
                }
                match first {
                    Some(Hit::Key(k)) if k.repeats() && now - t0 >= HOLD_MS && now - self.last_repeat >= REPEAT_MS => {
                        self.repeating = true;
                        self.last_repeat = now;
                        Some(Event::Held(k))
                    }
                    _ => None,
                }
            }
            (None, Some((first, _))) => {
                self.start = None;
                if self.repeating {
                    Some(Event::Release)
                } else if self.cancelled {
                    None
                } else {
                    first.map(Event::Tap).or(Some(Event::Cancel))
                }
            }
            (None, None) => None,
        }
    }
}

/// A physical button with the same hold/repeat rules as on-screen buttons.
pub struct HeldKey {
    key: Key,
    down_at: Option<u64>,
    last_repeat: u64,
    repeating: bool,
}

impl HeldKey {
    pub fn new(key: Key) -> Self {
        HeldKey { key, down_at: None, last_repeat: 0, repeating: false }
    }

    pub fn update(&mut self, pressed: bool, now: u64) -> Option<Event> {
        match (pressed, self.down_at) {
            (true, None) => {
                self.down_at = Some(now);
                self.repeating = false;
                Some(Event::Press(self.key))
            }
            (true, Some(t0)) if self.key.repeats() && now - t0 >= HOLD_MS && now - self.last_repeat >= REPEAT_MS => {
                self.repeating = true;
                self.last_repeat = now;
                Some(Event::Held(self.key))
            }
            (false, Some(_)) => {
                self.down_at = None;
                self.repeating.then_some(Event::Release)
            }
            _ => None,
        }
    }
}

/// The single BOOT button: short press = Select (play/pause), long press = Menu.
/// Fires on release for a short press, so a long press never also plays.
#[derive(Default)]
pub struct Button {
    down_at: Option<u64>,
    fired_long: bool,
}

impl Button {
    pub fn update(&mut self, pressed: bool, now: u64) -> Option<Event> {
        match (pressed, self.down_at) {
            (true, None) => {
                self.down_at = Some(now);
                self.fired_long = false;
                None
            }
            (true, Some(t0)) if !self.fired_long && now - t0 >= LONG_MS => {
                self.fired_long = true;
                Some(Event::Press(Key::Menu))
            }
            (false, Some(_)) => {
                self.down_at = None;
                (!self.fired_long).then_some(Event::Press(Key::Select))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const PLAY: Option<Hit> = Some(Hit::Key(Key::Select));
    const BACK: Option<Hit> = Some(Hit::Key(Key::Left));

    fn feed(t: &mut Touch, seq: &[(Option<Option<Hit>>, u64)]) -> Vec<Event> {
        seq.iter().filter_map(|(f, now)| t.update(*f, *now)).collect()
    }

    #[test]
    fn tap_acts_on_release() {
        let ev = feed(&mut Touch::default(), &[(Some(PLAY), 0), (Some(PLAY), 50), (None, 90)]);
        assert_eq!(ev, [Event::Down(PLAY), Event::Tap(Hit::Key(Key::Select))]);
    }

    #[test]
    fn sliding_off_cancels() {
        let ev = feed(&mut Touch::default(), &[(Some(PLAY), 0), (Some(None), 50), (Some(PLAY), 60), (None, 90)]);
        assert_eq!(ev, [Event::Down(PLAY), Event::Cancel]);
    }

    #[test]
    fn empty_space_does_nothing() {
        let ev = feed(&mut Touch::default(), &[(Some(None), 0), (None, 900)]);
        assert_eq!(ev, [Event::Down(None), Event::Cancel]);
    }

    #[test]
    fn holding_back_repeats_then_releases() {
        let ev = feed(&mut Touch::default(), &[(Some(BACK), 0), (Some(BACK), 450), (Some(BACK), 500), (Some(BACK), 600), (None, 650)]);
        assert_eq!(ev, [Event::Down(BACK), Event::Held(Key::Left), Event::Held(Key::Left), Event::Release]);
    }

    #[test]
    fn holding_play_never_repeats() {
        let ev = feed(&mut Touch::default(), &[(Some(PLAY), 0), (Some(PLAY), 2000), (None, 2100)]);
        assert_eq!(ev, [Event::Down(PLAY), Event::Tap(Hit::Key(Key::Select))]);
    }

    #[test]
    fn physical_direction_key_repeats() {
        let mut k = HeldKey::new(Key::Up);
        assert_eq!(k.update(true, 0), Some(Event::Press(Key::Up)));
        assert_eq!(k.update(true, 450), Some(Event::Held(Key::Up)));
        assert_eq!(k.update(false, 500), Some(Event::Release));
    }

    #[test]
    fn boot_button_short_and_long() {
        let mut b = Button::default();
        assert!(b.update(true, 0).is_none());
        assert_eq!(b.update(false, 100), Some(Event::Press(Key::Select)));
        assert!(b.update(true, 200).is_none());
        assert_eq!(b.update(true, 1000), Some(Event::Press(Key::Menu)));
        assert!(b.update(false, 1100).is_none(), "long press fires once, release is silent");
    }
}

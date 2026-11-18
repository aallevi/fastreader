//! The one input vocabulary every platform maps onto.
//!
//! Physical buttons, on-screen buttons, keyboard arrows and the web page all
//! end up as a [`Key`]. Each key means the same thing on every screen:
//!
//! | Key     | Meaning everywhere                                   |
//! |---------|------------------------------------------------------|
//! | Up/Down | change the value in focus (speed, page, selection)   |
//! | Left    | go back (previous sentence/page, back out of a list) |
//! | Right   | go forward (next sentence/page, open)                |
//! | Select  | the primary action (play/pause, choose)              |
//! | Menu    | open or close the menu, from anywhere                |

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Select,
    Menu,
}

impl Key {
    pub const ALL: [Key; 6] = [Key::Up, Key::Down, Key::Left, Key::Right, Key::Select, Key::Menu];

    /// Keys that repeat while held (speed, scrolling, lists).
    pub const fn repeats(self) -> bool {
        matches!(self, Key::Up | Key::Down | Key::Left | Key::Right)
    }
}

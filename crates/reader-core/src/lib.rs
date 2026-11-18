//! Platform-free logic for the fast reader: the same code runs on the ESP32,
//! in the desktop simulator and in tests.
#![no_std]
extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod app;
pub mod book;
pub mod chunked;
pub mod gestures;
pub mod input;
pub mod reader;
pub mod settings;

pub use app::{App, Catalog, Effect, Feedback, Hit, Msg, Screen, Tone};
pub use book::{Book, Style, Word};
pub use input::Key;
pub use settings::Settings;

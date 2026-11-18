//! Anti-aliased, themeable renderer for the fast reader. The ESP32 and the
//! simulator both draw into a [`Canvas`] and push the damaged rectangles.
#![no_std]
extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod canvas;
pub mod color;
pub mod geom;
pub mod icons;
pub mod layout;
pub mod painter;
pub mod text;
pub mod theme;
pub mod themes;

use reader_core::Catalog;

pub use canvas::Canvas;
pub use geom::Rect;
pub use painter::Ui;

/// Theme and font names for `reader_core::App`.
pub const CATALOG: Catalog = Catalog { themes: &themes::THEME_NAMES, fonts: &text::FONT_NAMES };

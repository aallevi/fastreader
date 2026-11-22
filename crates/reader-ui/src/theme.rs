//! A theme is plain data: colours, gradients, button styles, fonts and a
//! chrome style. Adding a look = one file in `themes/`, listed in `themes/mod.rs`.

use crate::color::Rgb;
use crate::text::Face;

pub type Stops = &'static [(u8, Rgb)];

/// Signature touches that colours alone can't express.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chrome {
    /// Windows Vista / Photo Gallery: glossy split gradients, aurora glass.
    Vista,
    /// Scrivener composition mode: paper column on a dark backdrop.
    Scrivener,
    /// Windows 98: grey embossed bevels, navy title bar, square corners.
    Win98,
}

/// How one kind of button looks.
#[derive(Clone, Copy, Debug)]
pub struct ButtonStyle {
    pub face: Stops,
    pub edge: Rgb,
    pub text: Rgb,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub chrome: Chrome,
    /// UI label faces: regular, bold.
    pub ui: (Face, Face),
    pub radius: f32,
    pub upper: bool,
    /// Light text on a dark page: draw glyph coverage linearly (no boost),
    /// because light-on-dark already looks heavier and blooms on an IPS panel.
    pub light_on_dark: bool,

    pub desktop: Stops,
    /// Title bar and bottom bar.
    pub bar: Stops,
    pub bar_text: Rgb,
    pub bar_dim: Rgb,

    pub page: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub heading: Rgb,
    /// Highlight behind the current word (line, context, page views).
    pub mark: Rgb,
    pub mark_text: Rgb,
    /// Guide lines above/below the word, and the ornate variant's colour.
    pub guide: Rgb,
    pub ornament: Rgb,

    /// Buttons on the bars, pressed buttons, the play button.
    pub button: ButtonStyle,
    pub button_hot: ButtonStyle,
    pub primary: ButtonStyle,
    pub primary_hot: ButtonStyle,
    /// Menu rows (buttons on the list pane) and the selected one.
    pub row: ButtonStyle,
    pub row_sel: ButtonStyle,

    pub list: Stops,
    pub section: Rgb,

    pub toast: Rgb,
    pub toast_edge: Rgb,
    pub toast_text: Rgb,
    pub limit: Rgb,
    pub limit_text: Rgb,

    pub track: Rgb,
    pub fill: Rgb,
}

impl Theme {
    /// Theme colours as CSS variables, so the web view matches the device.
    pub fn css_vars(&self) -> alloc::string::String {
        use core::fmt::Write;
        let mut s = alloc::string::String::from(":root{");
        let first = |st: Stops| st[0].1;
        let last = |st: Stops| st[st.len() - 1].1;
        for (name, c) in [
            ("bg", first(self.desktop)),
            ("bg2", last(self.desktop)),
            ("page", self.page),
            ("edge", self.track),
            ("text", self.text),
            ("dim", self.dim),
            ("heading", self.heading),
            ("bar", first(self.bar)),
            ("bar2", last(self.bar)),
            ("bartext", self.bar_text),
            ("accent", self.fill),
            ("mark", self.mark),
            ("marktext", self.mark_text),
            ("btn", first(self.row.face)),
            ("btnedge", self.row.edge),
            ("btntext", self.row.text),
            ("sel", first(self.row_sel.face)),
            ("seltext", self.row_sel.text),
        ] {
            let _ = write!(s, "--{name}:#{c:06x};");
        }
        let family = match self.chrome {
            Chrome::Vista | Chrome::Win98 => "'Segoe UI',Selawik,Tahoma,sans-serif",
            _ => "-apple-system,'IBM Plex Sans','Helvetica Neue',sans-serif",
        };
        let _ = write!(s, "--radius:{}px;--font:{family};}}", self.radius);
        s
    }
}

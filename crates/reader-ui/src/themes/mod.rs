//! All built-in looks. To add one: copy a file, change the data, list it here.
mod night;
mod scrivener;
mod vista;
mod win98;

use crate::theme::Theme;

/// Four looks. Windows 98 stays number 3 (the saved choice and the default).
pub static THEMES: [&Theme; 4] = [&vista::VISTA, &scrivener::SCRIVENER, &night::NIGHT, &win98::WIN98];

pub static THEME_NAMES: [&str; 4] = [vista::VISTA.name, scrivener::SCRIVENER.name, night::NIGHT.name, win98::WIN98.name];

pub fn theme(i: u8) -> &'static Theme {
    THEMES[i as usize % THEMES.len()]
}

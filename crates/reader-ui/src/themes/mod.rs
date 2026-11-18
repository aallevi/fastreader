//! All built-in looks. To add one: copy a file, change the data, list it here.
mod ink;
mod macos;
mod night;
mod scrivener;
mod vista;
mod win98;

use crate::theme::Theme;

pub static THEMES: [&Theme; 6] = [&vista::VISTA, &macos::MACOS, &scrivener::SCRIVENER, &win98::WIN98, &ink::INK_THEME, &night::NIGHT];

pub static THEME_NAMES: [&str; 6] = [vista::VISTA.name, macos::MACOS.name, scrivener::SCRIVENER.name, win98::WIN98.name, ink::INK_THEME.name, night::NIGHT.name];

pub fn theme(i: u8) -> &'static Theme {
    THEMES[i as usize % THEMES.len()]
}

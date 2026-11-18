//! Night / eye-strain mode, made for this IPS panel: no true black (its
//! black glows grey and only adds blooming), warm low-blue colours, text at
//! about 7:1 contrast instead of 15:1+, and a slightly lighter text column
//! with a visible edge so the margins stay clear in the dark.
use crate::text::{PLEX_SANS, PLEX_SANS_BOLD};
use crate::theme::*;

pub const NIGHT: Theme = Theme {
    name: "Night",
    chrome: Chrome::Scrivener,
    ui: (PLEX_SANS, PLEX_SANS_BOLD),
    radius: 5.0,
    upper: false,
    light_on_dark: true,
    desktop: &[(0, 0x131211), (255, 0x131211)],
    bar: &[(0, 0x1B1A18), (255, 0x171615)],
    bar_text: 0x9E9585,
    bar_dim: 0x6D665C,
    page: 0x24221F,
    text: 0xB9AE9C,
    dim: 0x7D7468,
    heading: 0xC4A774,
    mark: 0x3B362F,
    mark_text: 0xD8CBB4,
    guide: 0x3E3A34,
    ornament: 0x6B5F4C,
    button: ButtonStyle { face: &[(0, 0x2A2825), (255, 0x252320)], edge: 0x3E3A34, text: 0xB9AE9C },
    button_hot: ButtonStyle { face: &[(0, 0x3A3631), (255, 0x34302B)], edge: 0x55504A, text: 0xD8CBB4 },
    primary: ButtonStyle { face: &[(0, 0x4A4033), (255, 0x3F372C)], edge: 0x5A4E3E, text: 0xE0CFAE },
    primary_hot: ButtonStyle { face: &[(0, 0x584C3C), (255, 0x4C4234)], edge: 0x6A5C48, text: 0xEAD9B8 },
    row: ButtonStyle { face: &[(0, 0x24221F), (255, 0x211F1C)], edge: 0x36332D, text: 0xB9AE9C },
    row_sel: ButtonStyle { face: &[(0, 0x3B362F), (255, 0x36312A)], edge: 0x5A4E3E, text: 0xE0D3BB },
    list: &[(0, 0x1B1A18), (255, 0x1B1A18)],
    section: 0x8A7F6E,
    toast: 0x2C2925,
    toast_edge: 0x4A443C,
    toast_text: 0xD8CBB4,
    limit: 0x5A4326,
    limit_text: 0xF0D9A8,
    track: 0x33302B,
    fill: 0x8A7A5E,
};

//! Windows 98, the grey vintage way: teal desktop, an inactive-grey title
//! bar, #C0C0C0 face with//! embossed bevels (white/grey/black), square corners, tooltip-yellow tips.
use crate::text::{SELAWIK, SELAWIK_BOLD};
use crate::theme::*;

const FACE: Stops = &[(0, 0xC0C0C0)];

pub const WIN98: Theme = Theme {
    name: "Windows 98",
    chrome: Chrome::Win98,
    ui: (SELAWIK, SELAWIK_BOLD),
    radius: 0.0,
    upper: false,
    light_on_dark: false,
    desktop: &[(0, 0x008080), (255, 0x008080)],
    bar: &[(0, 0x808080), (255, 0xB4B4B4)],
    bar_text: 0xFFFFFF,
    bar_dim: 0xC0C0C0,
    page: 0xFFFFFF,
    text: 0x000000,
    dim: 0x808080,
    heading: 0x000000,
    mark: 0xC0C0C0,
    mark_text: 0x000000,
    guide: 0xC0C0C0,
    ornament: 0x808080,
    button: ButtonStyle { face: FACE, edge: 0x000000, text: 0x000000 },
    button_hot: ButtonStyle { face: FACE, edge: 0x000000, text: 0x000000 },
    primary: ButtonStyle { face: FACE, edge: 0x000000, text: 0x000000 },
    primary_hot: ButtonStyle { face: FACE, edge: 0x000000, text: 0x000000 },
    row: ButtonStyle { face: FACE, edge: 0x000000, text: 0x000000 },
    // Selection is a pushed-in button with a dotted focus rectangle: no colour.
    row_sel: ButtonStyle { face: FACE, edge: 0x000000, text: 0x000000 },
    list: &[(0, 0xC0C0C0), (255, 0xC0C0C0)],
    section: 0x000000,
    toast: 0xFFFFE1,
    toast_edge: 0x000000,
    toast_text: 0x000000,
    limit: 0xFFFFE1,
    limit_text: 0x800000,
    track: 0xC0C0C0,
    fill: 0x404040,
};

//! E-ink, like the original project's device: flat paper grey, black
//! hairlines, uppercase mono labels, outlined buttons that invert when pressed.
use crate::color::Rgb;
use crate::text::{MONO, MONO_BOLD};
use crate::theme::*;

const PAPER: Rgb = 0xE9E9E4;
const INK: Rgb = 0x111111;

pub const INK_THEME: Theme = Theme {
    name: "E-Ink",
    chrome: Chrome::Ink,
    ui: (MONO, MONO_BOLD),
    radius: 3.0,
    upper: true,
    light_on_dark: false,
    desktop: &[(0, PAPER), (255, PAPER)],
    bar: &[(0, PAPER), (255, PAPER)],
    bar_text: INK,
    bar_dim: 0x555555,
    page: PAPER,
    text: INK,
    dim: 0x666666,
    heading: INK,
    mark: INK,
    mark_text: PAPER,
    guide: 0x333333,
    ornament: INK,
    button: ButtonStyle { face: &[(0, PAPER)], edge: INK, text: INK },
    button_hot: ButtonStyle { face: &[(0, INK)], edge: INK, text: PAPER },
    primary: ButtonStyle { face: &[(0, INK)], edge: INK, text: PAPER },
    primary_hot: ButtonStyle { face: &[(0, 0x555555)], edge: INK, text: PAPER },
    row: ButtonStyle { face: &[(0, PAPER)], edge: INK, text: INK },
    row_sel: ButtonStyle { face: &[(0, INK)], edge: INK, text: PAPER },
    list: &[(0, PAPER), (255, PAPER)],
    section: INK,
    toast: INK,
    toast_edge: INK,
    toast_text: PAPER,
    limit: 0x777777,
    limit_text: PAPER,
    track: 0xBDBDB6,
    fill: INK,
};

//! Windows Vista, Photo Gallery style (colours from Scan Studio's vista.css):
//! dark glossy command bars with a hard mid-split, white Explorer pane,
//! glossy grey command buttons, a blue glass play button.
use crate::text::{SELAWIK, SELAWIK_BOLD};
use crate::theme::*;

const CMD: Stops = &[(0, 0x3D6584), (122, 0x24435D), (124, 0x0F2A3F), (255, 0x0E2537)];

pub const VISTA: Theme = Theme {
    name: "Vista",
    chrome: Chrome::Vista,
    ui: (SELAWIK, SELAWIK_BOLD),
    radius: 3.0,
    upper: false,
    light_on_dark: false,
    desktop: &[(0, 0x0A2F45), (128, 0x0F5C6E), (255, 0x082233)],
    bar: CMD,
    bar_text: 0xFFFFFF,
    bar_dim: 0xC9D8E6,
    page: 0xFFFFFF,
    text: 0x1E1E1E,
    dim: 0x5A6779,
    heading: 0x1E3287,
    mark: 0xCDE8FB,
    mark_text: 0x1E1E1E,
    guide: 0xB9CCE0,
    ornament: 0x3D6584,
    button: ButtonStyle { face: &[(0, 0x58799A), (124, 0x3A5C7C), (126, 0x25445F), (255, 0x2E5576)], edge: 0x0B1B28, text: 0xFFFFFF },
    button_hot: ButtonStyle { face: &[(0, 0xA9DCFF), (124, 0x5FB0E6), (126, 0x2E8BD3), (255, 0x4EA8E8)], edge: 0x0B2236, text: 0xFFFFFF },
    primary: ButtonStyle { face: &[(0, 0x8AD0F5), (122, 0x2A8AD1), (128, 0x0B5AA7), (255, 0x1478C6)], edge: 0x0B2236, text: 0xFFFFFF },
    primary_hot: ButtonStyle { face: &[(0, 0xB4E4FB), (122, 0x5AB0E8), (128, 0x2A7CC8), (255, 0x3D9BE0)], edge: 0x0B2236, text: 0xFFFFFF },
    row: ButtonStyle { face: &[(0, 0xF6F6F6), (124, 0xEDEDED), (126, 0xE0E0E0), (255, 0xD5D5D5)], edge: 0x8E8F8F, text: 0x1E1E1E },
    row_sel: ButtonStyle { face: &[(0, 0xEAF6FD), (124, 0xD9F0FC), (126, 0xBEE6FD), (255, 0xA7D9F5)], edge: 0x3C7FB1, text: 0x1E1E1E },
    list: &[(0, 0xFDFEFF), (255, 0xEEF3FA)],
    section: 0x1E3287,
    toast: 0xEEF4FB,
    toast_edge: 0x99DEFD,
    toast_text: 0x1E3287,
    limit: 0xF4C25A,
    limit_text: 0x3D2A00,
    track: 0xD5DFE9,
    fill: 0x3399FF,
};

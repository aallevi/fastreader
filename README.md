# Fast Reader

A one-word-at-a-time (RSVP) speed reader for the **LCDWIKI ES3C28P** (ESP32-S3, 2.8" 320×240 IPS, touch), written in Rust. Inspired by [AllanBinder/ereader](https://github.com/AllanBinder/ereader); see [AUDIT.md](AUDIT.md) for how the two compare and how every function was checked.

- Words centred between guide lines; chapter titles bold and on screen longer; bold and italic kept.
- Speed view and page view; hold ⏮/⏭ to scroll the text as a line.
- Library of several books, each remembering where you stopped.
- Five themes (Vista, macOS, Scrivener, Windows 98, E-Ink), six reading fonts, anti-aliased.
- A browser view of the whole book plus every setting (simulator now, device over Wi-Fi next).

## Controls: one meaning per key, on every screen

| On screen | Physical key | Reading | Page view | Menus |
|---|---|---|---|---|
| ☰ (top-left) | BOOT long press | menu | menu | close |
| − / + | ▼ / ▲ | speed ∓25 wpm (hold repeats) | page ∓1 | move |
| ⏮ / ⏭ | ◀ / ▶ | jump back/forward (amount in menu) · **hold: scroll line** | page ∓1 | back / open |
| ▶ ❚❚ | BOOT short press | play / pause | speed-read from here | choose |

A tap acts on release, sliding off cancels, and there are no swipes or invisible zones. Every press lights its button and shows a short message in the title bar (amber means a limit, like the fastest speed). Info appears only while paused.

## Books

```sh
python3 -m venv tools/.venv && tools/.venv/bin/pip install pymupdf
tools/.venv/bin/python tools/convert.py book.epub paper.pdf notes.txt   # → books/private/*.md
```
The converter keeps chapters (table of contents, larger or all-bold lines, EPUB headings), bold and italic, and drops running headers, page numbers and hyphenation. `books/private/` is git-ignored. The firmware embeds every book in `books/` and `books/private/` at build time.

## Run

```sh
cargo run -p reader-sim --release                 # window + browser view at http://localhost:8080
cargo run -p reader-sim --release -- walkthrough out/   # PNG of every function in every theme
cargo test --workspace                            # 87 tests: logic, layout, rendering, efficiency
cargo bench -p reader-ui

# device (espup + espflash installed)
. ~/export-esp.sh
cd firmware && cargo run --release                # add --features selftest to log load/paint timings
```

`backup/es3c28p-photo-frame-full-flash.bin` restores the photo-frame firmware: `espflash write-bin 0 backup/es3c28p-photo-frame-full-flash.bin`.

## Layout

| Path | What |
|---|---|
| `crates/reader-core` | `no_std` logic: books and markup, pacing, keys → actions, feedback, touch gestures, settings |
| `crates/reader-ui` | `no_std` renderer: canvas, TrueType text, themes, layout + hit testing, painter |
| `crates/reader-sim` | desktop window, browser view and settings server, walkthrough |
| `firmware` | ES3C28P: display, touch, buttons, backlight, flash storage, embedded books |
| `tools/convert.py` | EPUB/PDF/TXT → reader markup |

Add a theme: copy a file in `crates/reader-ui/src/themes/`, change colours and button styles, list it in `themes/mod.rs`.

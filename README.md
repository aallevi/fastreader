# Fast Reader

A one-word-at-a-time (RSVP) speed reader for the **LCDWIKI ES3C28P** (ESP32-S3, 2.8" 320×240 IPS, touch), written in Rust. Inspired by [AllanBinder/ereader](https://github.com/AllanBinder/ereader); see [AUDIT.md](AUDIT.md) for how the two compare and how every function was checked.

- Words centred on a clean page; chapter titles bold and on screen longer; bold and italic kept.
- Hold ⏮/⏭ while reading to scroll the text as a line.
- Library of several books, each remembering where you stopped (saved in flash, survives power-off).
- **Bookmarks:** tap the page's top-right corner to dog-ear the place; the corner folds down with a soft shadow. Tap again to remove. Menu → Bookmarks lists them; the web view marks them too.
- **Pictures and charts** from EPUBs and PDFs (vector charts included) fill the screen for 4× a word's time; in page view they sit in the text.
- **Four menus in the title bar:** ⚙ Settings (speed, pace, skip, theme, brightness, rotation, Wi-Fi, info, sleep) · 📖 Book (resume, chapters, bookmarks, go to %, start over) · 📚 Library (the book list) · Aa Text (mode, size, font, words per flash, word position, spacing, paragraphs, alignment, parentheses). The ▲▼ keys run through all four in order.
- **Word position:** Centred (default) or Recognition point. With the recognition point, words over 13 letters come in two flashes split at a syllable ("contempo-" "raneamente"), as Spritz does, so the point never moves.
- **Picture time** (Aa Text): 2, 4, 8 or 15 seconds, or until you press play; counted from when the picture is on screen.
- **The sentence under the word** (while paused) follows the word; when it is longer than two lines, small ▲▼ beside it scroll through it.
- **Recognition point follows published practice:** one fixation point a little left of centre. A single word puts its optimal recognition letter there (Spritz patent US9483109, OpenSpritz, Jetzt: 2nd letter of 2–5-letter words, 3rd of 6–9, 4th of 10–13), uncoloured. A phrase starts about 3½ letters before it, since readers take in ~4 letters left and ~15 right of a fixation (McConkie & Rayner), which is why the original e-reader uses a fixed left edge. Text never changes size; only a group wider than the screen shrinks, and a long one starts further left.
- **⏮ like a music player:** a second press within 2.5 s goes one more sentence/phrase back, even if playing moved on meanwhile.
- **Brackets** — round ( ), square [ ], curly { }, angle < > and << >>, guillemets « » ‹ › — read as punctuation: a short breath as an aside opens and closes. Each closing bracket is matched with its opening one across the whole book, even when a page break splits the aside. Optionally the brackets (of the right kind) become marks at the screen's sides in word/phrase modes (Aa → Brackets).
- Six themes (Vista, macOS, Scrivener, Windows 98, E-Ink, Night), six reading fonts, anti-aliased.
- Four reading modes: word by word, phrases (fixed groups of 1–3 words that end at sentences, starting at the same left point, as on the original e-reader), ticker (the line steps word by word like a typewriter carriage; nothing glides), page (normal text with a moving phrase highlight).
- **microSD card:** books and every picture live on the card (any size; a scanned art book with 368 illustrations is ~31 MB). ⚙ Settings → SD card shows the free space and can **Erase and format** it (FAT32; the reader can't read exFAT, which most cards over 32 GB and many used cards have). Without a card, books go to the reader's own flash (3 MB each, pictures shrunk to fit).
- **Big scanned books** (hundreds of pages of page photos) are too heavy for a phone's browser (over 1 GB of memory): the phone says so and suggests a computer, either the same page at http://192.168.4.1 in a desktop browser, or `tools/convert.py --sd` with the card in the computer (Gombrich, 702 pages: 8 s, 422 pictures).
- **Add books from the phone:** EPUB, PDF, Markdown or text, converted in the phone's browser (chapters, bold/italic, pictures, running heads and page numbers removed, page-split sentences rejoined). pdf.js is served by the reader itself, so no internet is needed.
- A browser view of the book plus every setting: on the device, Menu → Web view, join Wi-Fi "FastReader", open http://192.168.4.1.

## Controls: one meaning per key, on every screen

| On screen | Physical key | Reading | Page view | Menus |
|---|---|---|---|---|
| ⚙ 📖 📚 Aa (top-left) | BOOT long press (opens 📖) | Settings · Book · Library · Text menus | same | close |
| − / + | ▼ / ▲ | speed ∓25 wpm (hold repeats) | speed ∓25 wpm | move |
| ⏮ / ⏭ | ◀ / ▶ | jump back/forward (amount in menu) · **hold: scroll line** | previous / next page (hold repeats) | back / open |
| page corner (top-right) | – | bookmark here / remove | bookmark this page / remove | – |
| ▶ ❚❚ | BOOT short press | play / pause | play / pause the moving highlight | choose |

**Optional physical buttons:** wire momentary switches from these pins to GND (no resistors needed): GPIO 2 = ▲, GPIO 3 = ▼, GPIO 14 = ◀, GPIO 21 = ▶. They behave exactly like the on-screen buttons, including hold-to-repeat and hold-to-scroll; BOOT stays select (short) / menu (long).

A tap acts on release, sliding off cancels, and there are no swipes or invisible zones. Every press lights its button and shows a short message in the title bar (amber means a limit, like the fastest speed). Info appears only while paused.

## Books

```sh
python3 -m venv tools/.venv && tools/.venv/bin/pip install pymupdf
tools/.venv/bin/python tools/convert.py book.epub paper.pdf notes.txt   # → books/private/*.md
tools/.venv/bin/python tools/convert.py big-scan.pdf --sd /Volumes/FASTREADER   # straight onto the reader's SD card, every picture kept
```
### Book format

Plain text with a few marks:

| Mark | Meaning |
|---|---|
| `# Title` | chapter title: bold, on screen longer (2× by default) |
| `**bold**` `*italic*` | kept as bold / italic |
| blank line | new paragraph (longer pause) |
| `{pause}` / `{pause:3}` | an extra beat (or 3) after the previous word, for dramatic effect |
| `{slow}` … `{/slow}` | that passage at 1.4× the time |
| `{verse}` … `{/verse}` | poetry: each line break is kept on the page and gets a breath |
| `{image:N}` | picture N from the image section after the text (written by the converters) |

The marks follow the spirit of SSML (the W3C speech-timing standard: `<break>`, `<prosody rate="slow">`) in a form that stays readable as plain text.

The converter keeps chapters (table of contents, larger or all-bold lines, EPUB headings), bold and italic, and drops running headers, page numbers and hyphenation. `books/private/` is git-ignored. The firmware embeds every book in `books/` and `books/private/` at build time.

## Run

```sh
cargo run -p reader-sim --release                 # window + browser view at http://localhost:8080
cargo run -p reader-sim --release -- --headless   # only the browser view
cargo run -p reader-sim --release -- walkthrough out/   # PNG of every function in every theme
cargo test --workspace                            # 129 tests: logic, layout, rendering, efficiency
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
| `firmware` | ES3C28P: display, touch, buttons, backlight, flash storage, SD card, embedded books |
| `crates/fat-format` | FAT32 formatter for the SD card (`no_std`), checked against macOS `fsck_msdos` |
| `tools/convert.py` | EPUB/PDF/TXT → reader markup |

Add a theme: copy a file in `crates/reader-ui/src/themes/`, change colours and button styles, list it in `themes/mod.rs`.

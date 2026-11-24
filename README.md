# Fast Reader: one word at a time

A speed reader (RSVP) built on a small ESP32-S3 touch screen, written in
Rust. The same reader runs on the device, as a Mac app and in a browser.
RSVP readers turn a book into a stream of words and leave the rest behind;
Fast Reader keeps pictures, figures, lists and tables in the reading, each
shown the way it works best one word at a time, like no other fast reader.

**192** tests · **13.7 ms** per word frame on the device · **4** themes ·
EPUB, PDF, Markdown, text · [Mac app](https://github.com/aallevi/fastreader/releases/latest)

<img src="docs/themes.png" alt="The four themes: Windows 98, Vista, Scrivener, Night" width="340">

![Sonnet 18, word by word, in the Windows 98 theme](docs/sonnet-18.gif)

*Shakespeare's Sonnet 18 at 350 words a minute in the Windows 98 theme: near the end it pauses, jumps one sentence back, steps two words forward, bookmarks that word (the folded corner), and reads on to the end. Recorded from the simulator (`reader-sim record … --demo`).*

## What it does

### Reading

- Four ways to read: word by word, phrases (up to three words, ending at sentences), a typewriter ticker, and normal pages with a moving highlight.
- Words centred or held on a fixed recognition point (the Spritz rule), with optional guide lines and ticks at the letter the eye should land on.
- Natural pacing: longer pauses at commas, sentences, paragraphs and chapter titles; extra time for long words; an easy start after play; asides in brackets read as asides.
- Speed in steps of 25 wpm (default 475); ⏮ ⏭ jump by sentence, ‹ › step one word; bookmarks as a folded page corner; chapters, go to, start over.
- A welcome back card after a break or at power-on: the book, author, chapter and how far in; play restarts at the start of the sentence.

### The whole book, not just its words

- **Pictures** come in the stream for a set time (2 to 15 seconds, or until play); their description shows when paused.
- **Figure references:** at "see figure 11" the figure itself appears below the word; numbers are learnt from the captions when the book opens.
- **Lists:** bullets or numbers down the side with the current item lit, a breath between items; pictures in lists and catalogues (picture, then "1. title") stay with their item.
- **Tables:** the whole table below the word with the current cell lit, or a quick grid before each cell; half speed with a breath before each column; a table is one sentence for ⏮ ⏭ and a cell one word for ‹ ›.

### Books and library

- The library lives on the SD card (books and their pictures); the reader formats the card and copes with it being taken out: a warning, the place kept, the library rebuilt when it comes back.
- Converters for EPUB, PDF (scanned ones too), Markdown and text, on the phone and on the Mac; the Mac converter also recovers tables and symbol-font bullets from PDFs, and an OCR tool turns scanned books into EPUBs.
- Books added from the phone page by choosing or dropping a file; deleting with a confirmation.

### Phone page, Mac and browser

- Web view: join the reader's Wi-Fi and the open book appears in the phone's browser, following your place; every setting, the library and adding books are there too.
- The Mac app (DMG, Apple silicon and Intel) keeps books and places between versions and says when a new version is out, with its changes.
- A browser demo (WebAssembly) that fills any screen or shows a chosen display's resolution, and a desktop simulator for development.

### Look and comfort

- Themes: Windows 98, Vista, Scrivener, Night; optional colourful menu buttons; menus in levels with Theme and Web view on top.
- Sleep timer (default 10 minutes), dim when idle, Web view off when unused, the battery level in green, orange or red.

## Hardware

One ready-made board, no soldering needed for reading.

| | |
|---|---|
| **Board** | LCDWIKI ES3C28P, 2.8" ESP32-S3 display module |
| **Processor** | ESP32-S3 N16R8, dual-core, run at 160 MHz; 16 MB flash, 8 MB PSRAM (octal, 80 MHz) |
| **Screen** | 2.8" IPS, 320 × 240, ILI9341V over SPI; PWM backlight |
| **Touch** | FT6336G capacitive (I²C 0x38), with an interrupt line |
| **Storage** | microSD in SPI mode (20 MHz), FAT32 (the reader formats it); books and pictures live on the card |
| **Wireless** | 2.4 GHz Wi-Fi: its own network "FastReader" and the home network; Bluetooth LE only (no Bluetooth audio) |
| **Power** | USB-C 5 V; TP4054 charger, charges the battery while reading; battery sense on GPIO 9 (to calibrate) |
| **Battery** | 3.7 V LiPo with protection, 1.25 mm 2-pin plug (check polarity), 2000–3000 mAh |
| **Buttons** | BOOT (play, hold for menu); optional buttons on GPIO 2, 3, 14, 21 to ground |
| **Audio (unused yet)** | ES8311 codec, FM8002E amplifier (1.5 W into 8 Ω) on a mono speaker connector, MEMS microphone; no headphone jack |

Speed on the device (selftest, steady reading): a word frame takes 13.7 ms
median at 160 MHz (11.7 ms at 240), phrases 18 ms, ticker 23 ms, page turns
33 ms; a word stays about 126 ms on screen at 475 wpm.

## Power

Estimates from the parts' typical figures, to be replaced by measurements.

| Part | Draw (estimate) | What the reader does about it |
|---|---|---|
| Wi-Fi radio | 100–180 mA | Off until Web view is chosen; switches itself off after 10 minutes unused |
| Backlight | 20–60 mA | Dims after a minute idle; brightness in five steps (dark themes don't save on an LCD) |
| Processor | 40–60 mA | 160 MHz instead of 240; light sleep between words planned |
| Touch, card, rest of the board | a few mA | Deep sleep after the sleep timer; a power switch for storage |

With a 2000–3000 mAh battery: roughly 20–35 hours of reading with Wi-Fi
off, 10–15 with it on. Deep sleep keeps the place; BOOT or a touch wakes it.

## Roadmap

In order, shipped as 0.2.x updates (details in [PLAN.md](PLAN.md)):

1. **Battery** (mostly done): measurements, calibration, light sleep between words and a power switch wait for the battery.
2. **Articles and RSS feeds** (next): web links sent from the phone, the Mac or the Share sheet become clean articles; feeds by link or OPML import, fetched by the reader itself over home Wi-Fi or a hotspot; ⏭ at the last word goes to the next unread article; everything on the SD card, with "Clear old articles".
3. **The Mac app as reader and readers manager**: pair your physical readers, see each one's space and books, sync your own library, articles and Zotero to each, places followed across readers; RSS subscriptions imported and exported as OPML.
4. **Try it without the device** (next): the browser demo on GitHub Pages, EPUB and PDF in the demo, a one-click web flasher.
5. **The reader speaks**: a small speaker or wired earbuds; MP3 audiobooks or speech made on the Mac, with the words following the voice.
6. **Library, new look**: author, progress, last opened, audio or a second language.
7. **Lists and tables, the rest**: page view indents and table grids; tables in the phone's PDF converter.
8. **Read state**: unread, reading, finished.
9. **Zotero and server library** over Tailscale.
10. **Ideas for after**: bilingual books side by side, DRM-free Apple Books and Kindle imports, moving backgrounds, face-down to pause, a clock.

## The code, by size

```
┌──────────────────────────────────────┬──────────────────────────────────┐
│ reader-core  38%   logic, no_std     │ reader-ui  30%   drawing, no_std │
│ ┌─────────────────┬────────────────┐ │ ┌───────────────┬──────────────┐ │
│ │ app.rs          │ book.rs        │ │ │ painter.rs    │ layout.rs    │ │
│ │ keys → actions, │ markup, pacing,│ │ │ every screen, │ buttons and  │ │
│ │ menus, feedback │ brackets, pics │ │ │ redraw boxes  │ touch areas  │ │
│ ├──────────┬──────┴─────┬──────────┤ │ ├───────────────┼──────────────┤ │
│ │ reader.rs│ settings.rs│ gestures │ │ │ web.rs        │ canvas, text │ │
│ │ timing   │ by name    │ chunked  │ │ │ phone server  │ themes, icons│ │
│ └──────────┴────────────┴──────────┘ │ └───────────────┴──────────────┘ │
├───────────────────┬──────────┬───────┴──┬───────────┬───────────────────┤
│ firmware  12%     │ sim  6%  │ tools 6% │ web  6%   │ fat-format  2%    │
│ ESP32-S3: display,│ desktop  │ convert, │ the phone │ formats the SD    │
│ touch, SD, Wi-Fi  │ + tests  │ OCR→EPUB │ page      │ card as FAT32     │
└───────────────────┴──────────┴──────────┴───────────┴───────────────────┘
```

Everything that decides what you see is in the two `no_std` crates, tested on
the desktop (192 tests, including randomized stress runs and a book with every reading feature played through every theme and mode); the firmware only
moves pixels, touches and bytes.

## Controls

| On screen | Key | Reading | Page view |
|---|---|---|---|
| ⚙ 📖 📚 Aa | BOOT long | Settings · Book · Library · Text | same |
| − / + | ▼ / ▲ | speed ∓25 wpm | same |
| ⏮ / ⏭ | ◀ / ▶ | back / forward a sentence · hold: scroll | previous / next page |
| ‹ / › | – | one word, and pause | same |
| ▶ | BOOT short | play / pause | same |
| page corner | – | bookmark | same |

## Try it

**On a Mac:** download `FastReader-<version>.dmg` from the
[releases](https://github.com/aallevi/fastreader/releases/latest), open it
and drag **Fast Reader** onto Applications. Add books from its page at
http://localhost:8080 (Settings > Add a book); they stay, with your place,
in ~/Library/Application Support/FastReader. The app isn't signed with an
Apple Developer ID yet: the first time, right-click it > Open (or System
Settings > Privacy & Security > Open Anyway).

**On the reader:** `espflash write-bin 0x10000 fastreader-es3c28p-<version>-update.bin`
updates it (settings, places and books stay); `espflash write-bin 0 fastreader-es3c28p-<version>-full.bin`
is for a first install.

**Releases:** `packaging/release.sh <version> <notes.md>` tests, tags, builds
the Mac app and both firmware images, and publishes them together; the notes'
bullet points are what the Mac app's update notice lists.

## Run it

```sh
cargo test --workspace                                   # logic, drawing, web, stress
tools/build_demo.sh                                      # the reader in a web page (WebAssembly): docs/demo
cargo run -p reader-sim --release                        # desktop window (sharp, resizable: fills any size) + web page on :8080
cargo run -p reader-sim --release -- --device            # the reader's real 320x240 pixels
cargo run -p reader-sim --release -- --lan               # let a phone on the same Wi-Fi open :8080 (allow it in the macOS firewall)
cargo run -p reader-sim --release -- walkthrough out/    # a PNG of every screen and theme

. ~/export-esp.sh && cd firmware                         # the device (espup, espflash)
cargo run --release
```

Books: `tools/convert.py book.epub scan.pdf --sd /Volumes/FASTREADER` writes
them straight to the card; `tools/ocr_book.py scan.pdf book.epub` turns a
scanned PDF into a real EPUB (LightOnOCR-2, text plus every illustration).
Or add them from the phone page.

## More

[VERIFY.md](VERIFY.md) on-device checklist · [AUDIT.md](AUDIT.md) how
everything was checked · [PLAN.md](PLAN.md) what's next

Inspired by [AllanBinder/ereader](https://github.com/AllanBinder/ereader);
built on esp-rs, Embassy, pdf.js and more, and on reading research from
McConkie & Rayner to Rayner et al. (2016): see **[CREDITS.md](CREDITS.md)**
for credits and bibliography. Code MIT ([LICENSE](LICENSE)); fonts SIL OFL 1.1 ([assets/fonts](assets/fonts/LICENSE.md));
pdf.js Apache-2.0 ([assets/web](assets/web)).

# Plan

## Done
- Reader core: markup (chapters, bold, italic), pacing with adjustable pauses, words per flash, skip amount, chapters, go-to, page view, library with per-book positions, settings record with checksum.
- Interface: 4 themes (Windows 98, Vista, Scrivener, Night; macOS and E-Ink retired 2026-11-22); anti-aliased TrueType text; menu top-left; − speed + then ⏮ ▶ ⏭; rounded/bevelled buttons; info only when paused; messages in the title bar; no sliding UI.
- Device (LCDWIKI ES3C28P): display, touch, BOOT button, PWM brightness, rotation, settings + positions saved to flash, octal PSRAM at 80 MHz, books embedded at build time.
- Tools: `tools/convert.py` (EPUB/PDF/TXT → markup), desktop simulator with web view and settings, scripted walkthrough, tests and benchmarks. See [AUDIT.md](AUDIT.md).

## Done since
- Reading modes: Word, Phrases (meaning-based groups, fixed left start), Ticker (smooth glide), Page (typography options, moving phrase highlight, highlight dot).
- Web view on the device: Wi-Fi access point "FastReader" → http://192.168.4.1 (Menu → Web view), books stream in pieces, every setting editable.

- Bookmarks (dog-ear corner), positions per book in flash, page view steps phrase by phrase, uploads from the phone (EPUB converted in the browser, pictures included), captive portal, deep sleep, pictures and vector charts, brackets as punctuation with aside speed and optional side marks, typewriter ticker.

- Captive portal, PDF upload from the phone (pdf.js), microSD library (SPI mode), card formatting, captions, guide lines.
- Lists and tables: bulleted and numbered lists (•, *, +, 1., 1), a), iv.) with their markers down the left margin, the current one lit amber, a breath between items; tables (| a | b |, header from |---|) read row by row with the grid flashed before each cell, the cell about to be read in amber. Converters turn EPUB <ul>/<ol>/<table> into these.
- Home Wi-Fi: saved from the web page, joined alongside the FastReader network, `http://fastreader.local` by multicast DNS, rejoins on its own.
- Welcome back card (at power-on and after a set break, off/10 min/20 min/1 h/3 h; play restarts at the sentence start), the open book at the top of the Book menu, authors from the converters, drag and drop on the web page.

## Next
1. **Try it without the device**: the browser demo is built (docs/demo: the reader compiled to WebAssembly, mouse/touch/keys, settings and added EPUB/PDF/text books kept, fills the page or shows a chosen display's resolution); next, publish it on GitHub Pages, EPUB/PDF in the demo (the phone page's converters), and a one-click web flasher with ready firmware for people who have the board.
2. **The reader speaks** (audio add-on). The board has an ES8311 codec and an FM8002E amplifier (1.5 W into 8 Ω) wired to a mono speaker connector (JP3); there is **no headphone jack**, and the ESP32-S3 has no Bluetooth Classic, so no Bluetooth earbuds.
   - Speaker: a small 8 Ω speaker on JP3; the amplifier is enabled by pulling its SHUTDOWN pin low.
   - Earbuds, wired: a 3.5 mm jack on the speaker output through a resistor attenuator (mono in both ears; the output is bridged, so neither wire goes to ground), or a small I2S DAC board with its own jack if free pins allow.
   - Earbuds, wireless: through the phone page. **Done:** 🔊 Read aloud on the phone page: the phone's voice reads sentence by sentence from the reader's place, at the reader's speed, and the reader shows each word as it is spoken.
   - Sound: MP3 audiobooks from the card, or speech generated on the Mac, with word timings so the screen follows the voice.
3. **Library, new look** (after the audio add-on): a clearer list showing each book's author, progress, last opened, and whether it has audio or a second language.
4. **Lists and tables**: done in word and phrase mode, the converters and the phone page's book view (see Done). Still to do: page view indents list items with their markers and lays tables out as a grid; PDF tables (only EPUB/markup tables are recognised).
5. **Read state for books**: unread / reading / finished. Reaching the end offers "Mark as finished"; it can also be toggled from the Book menu and from the Library (and the phone page); finished books are marked in the list and their progress kept.
6. **Zotero / server library**: browse and download books from the Asus server over Tailscale, convert on the server, send to the reader.
7. Battery reading calibration once a LiPo is connected (GPIO 9 currently reads full scale on USB).
8. Finnegans Wake (public domain in the EU since 2012; keep out of public repos until 2035 for the US).

## Later
- Procedural backgrounds that move slowly (fusing bubbles, merging colours) behind the page.
- Audio pacing via the ES8311 codec (soft tick per word/phrase, lower tone at sentence ends); optional SAM voice; Pink-Trombone-style tract for ambience.
- IMU: face-down to pause; RTC clock in the header; speaker click (ES8311) as optional feedback.
- News sync; games (as in the original project).

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
- Figure references ("see figure 11") show the figure below the word; numbers learnt from captions.
- SD card taken out / put back: a warning, a stand-in page, the library rebuilt.
- Mac app (DMG, packaging/make_dmg.sh) and one-command releases with the firmware image (packaging/release.sh).
- Welcome back card (at power-on and after a set break, off/10 min/20 min/1 h/3 h; play restarts at the sentence start), the open book at the top of the Book menu, authors from the converters, drag and drop on the web page.

## Next
1. **Battery: make it last.** (Light sleep, measurements and calibration wait for the battery.) Rough budget (to be measured): Wi-Fi 100-180 mA, backlight 20-60 mA (an LCD: dark themes don't save), processor at 240 MHz 40-60 mA, touch polling and card a few mA.
   - Done: the **sleep timer** (Settings > Sleep timer, default 10 minutes; deep sleep with the place saved, BOOT or a touch wakes it).
   - Done: the reader on the normal build (Wi-Fi off until Web view is chosen), dim when idle, Web view off when unused (10 minutes), the battery level in colour, and the processor at 160 MHz: measured on the reader (selftest, 7 s of steady reading), a word frame takes 13.7 ms median at 160 MHz against 11.7 ms at 240 (phrases 18, ticker 23, page turns 33 ms), well within a word's ~126 ms at 475 wpm.
   - Light sleep between words and while paused; the touch chip's low-power mode and its interrupt pin to wake, instead of polling 100 times a second.
   - Measure each state with a USB power meter and put the real numbers (hours of reading, days asleep) in the README.
   - True off: deep sleep still leaves the board's regulator, charger and touch chip drawing a few mA; a small slide switch on the battery lead for storage.
   - Calibrate the battery reading once a LiPo is connected (GPIO 9 reads full scale on USB).
2. **Articles and RSS feeds.** Read articles from the web and follow your feeds on the reader.
   - Web pages: send a link (from the phone page, the Mac app, or the phone's Share sheet) and the reader gets the article itself, without menus and ads (a "reader view" extraction: title, author, text, pictures, lists and tables), as a book in its own Articles shelf.
   - RSS/Atom feeds: subscribe by link, or import your subscriptions as an OPML file (most feed readers can export one). New items show up in an Articles list with their source and date; read ones are marked; old ones are cleared after a while.
   - Fetching: when the reader is on the home Wi-Fi, it checks the feeds itself (HTTPS on the ESP32, a few at a time to keep memory free); otherwise the phone page or the Mac app fetches and sends them.
   - Offline: articles are kept on the SD card, so they can be read anywhere.
   - Connecting: to fetch by itself the reader joins the home Wi-Fi or a phone hotspot automatically (saved networks, tried in turn), and switches the radio off again when done.
   - Like an RSS app: at the last word of an article, ▶▶ (or ⏭) goes straight to the next unread article.
   - Storage: everything except the firmware lives on the SD card (articles, feeds list, read marks); a "Clear old articles" button removes read and old feed items.
3. **The Mac app as reader and readers manager.** The Mac app is one reader among the others and the place that runs them all.
   - Finding a reader: first look for it on the home Wi-Fi (fastreader.local); if it isn't there, look for the reader's own network ("FastReader") and offer to join it for the moment needed, then go back to the home network.
   - Readers: find the physical readers on the home Wi-Fi (fastreader.local) or by USB, pair each once with a code shown on its screen, name them; see each one's battery, version, card space used and free, and its books.
   - Library: one own library on the Mac (books, articles, pictures); per reader, choose what it gets: everything, some shelves, or picked books; sync sends what's missing and removes what was taken off; bookmarks follow you between readers. Places: the readers have no clock, so when a book's place differs between two readers the app asks which to keep ("Mac: 48% · Reader: 41%. Continue from…"), never decides by itself.
   - Sources, each assigned to readers: an RSS/article feed (which reader gets which feeds), Zotero (collections to readers), folders on the Mac.
   - Feeds in and out: import subscriptions from another feed reader as OPML, and export them back as OPML, so feeds can move between this app and others (or be backed up).
   - Each reader's page (from its own Wi-Fi or the local network) keeps showing its own options and reader view; the manager uses the same routes the page uses today (books, upload, delete, settings).
   - Pictures per reader: books are converted for one screen (pictures at most 300×190, in the reader's 16-bit colours), so on a bigger screen (a larger reader, the Mac app full size, the browser demo filling a page) they look soft. The library keeps each book's original pictures, and sync makes a version sized for each reader's screen.
   - Flashing: the app flashes readers itself, no command line. Over USB (built in, the same work as espflash): first install of a new board, updates, and rescue of a reader that won't start, with progress shown; settings, places and books kept unless a fresh start is chosen.
   - Over Wi-Fi (OTA): updates sent to a paired reader, which checks the image and starts it only if it boots fine (else it falls back to the old one). This needs two program slots in the reader's flash layout (the program is ~2.8 MB, the flash 16 MB), a one-time change made over USB.
   - The app offers each reader the matching firmware when a new version is out, with its changes, like its own update notice.
   - Built with **Tauri** (decided 2026-11-24): a Rust program with web-page screens in a native Mac window (the system's web engine, ~10 MB, DMG, built-in updater). The reading view is the WebAssembly reader of the browser demo, the manager screens reuse the phone page's controls and converters, so the Mac app, the phone page and the demo look and behave alike.
   - Order: Articles/RSS (item 2) are built inside the manager, fetched and converted on the Mac and sent to each reader; the reader fetching by itself over Wi-Fi comes later.
4. **Try it without the device**: the browser demo is built (docs/demo: the reader compiled to WebAssembly, mouse/touch/keys, settings and added EPUB/PDF/text books kept, fills the page or shows a chosen display's resolution); next, publish it on GitHub Pages, EPUB/PDF in the demo (the phone page's converters), and a one-click web flasher with ready firmware for people who have the board.
5. **The reader speaks** (audio add-on). The board has an ES8311 codec and an FM8002E amplifier (1.5 W into 8 Ω) wired to a mono speaker connector (JP3); there is **no headphone jack**, and the ESP32-S3 has no Bluetooth Classic, so no Bluetooth earbuds.
   - Speaker: a small 8 Ω speaker on JP3; the amplifier is enabled by pulling its SHUTDOWN pin low.
   - Earbuds, wired: a 3.5 mm jack on the speaker output through a resistor attenuator (mono in both ears; the output is bridged, so neither wire goes to ground), or a small I2S DAC board with its own jack if free pins allow.
   - Earbuds, wireless: through the phone page. **Done:** 🔊 Read aloud on the phone page: the phone's voice reads sentence by sentence from the reader's place, at the reader's speed, and the reader shows each word as it is spoken.
   - Sound: MP3 audiobooks from the card, or speech generated on the Mac, with word timings so the screen follows the voice.
6. **Library, new look** (after the audio add-on): a clearer list showing each book's author, progress, last opened, and whether it has audio or a second language.
   - Not only books: articles saved from the web and RSS feed items live in the library too, in their own shelves (Books, Articles, each feed), with source, date and read state; unread first, "Clear old articles" at hand.
7. **Lists and tables**: done in word and phrase mode (tables below the word or flashed), navigation (a table is a sentence, a cell a word), the converters (EPUB lists and tables, PDF tables on the Mac) and the phone page's book view. Still to do: page view indents list items and lays tables out as a grid; PDF tables in the phone page's converter.
8. **Read state for books**: unread / reading / finished. Reaching the end offers "Mark as finished"; it can also be toggled from the Book menu and from the Library (and the phone page); finished books are marked in the list and their progress kept.
9. **Zotero / server library**: browse and download books from the Asus server over Tailscale, convert on the server, send to the reader.
10. Finnegans Wake (public domain in the EU since 2012; keep out of public repos until 2035 for the US).

## Later
- Procedural backgrounds that move slowly (fusing bubbles, merging colours) behind the page.
- Audio pacing via the ES8311 codec (soft tick per word/phrase, lower tone at sentence ends); optional SAM voice; Pink-Trombone-style tract for ambience.
- IMU: face-down to pause; RTC clock in the header; speaker click (ES8311) as optional feedback.
- News sync; games (as in the original project).

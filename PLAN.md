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

## Next
1. Wi-Fi: captive portal (phone opens the page automatically on joining), then home network + `fastreader.local`.
2. PDF upload from the phone (pdf.js in the browser); microSD library (4-bit SDMMC on this board).
3. **Zotero / server library**: browse and download books from the Asus server over Tailscale, convert on the server, send to the reader.
4. Battery reading calibration once a LiPo is connected (GPIO 9 currently reads full scale on USB).
5. Finnegans Wake (public domain in the EU since 2012; keep out of public repos until 2035 for the US).

## Later
- Procedural backgrounds that move slowly (fusing bubbles, merging colours) behind the page.
- Audio pacing via the ES8311 codec (soft tick per word/phrase, lower tone at sentence ends); optional SAM voice; Pink-Trombone-style tract for ambience.
- IMU: face-down to pause; RTC clock in the header; speaker click (ES8311) as optional feedback.
- News sync; games (as in the original project).

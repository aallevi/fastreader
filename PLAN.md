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

## Next
1. Wi-Fi: home network + `fastreader.local`.
2. **The reader speaks** (audio add-on). The board has an ES8311 codec and an FM8002E amplifier (1.5 W into 8 Ω) wired to a mono speaker connector (JP3); there is **no headphone jack**, and the ESP32-S3 has no Bluetooth Classic, so no Bluetooth earbuds.
   - Speaker: a small 8 Ω speaker on JP3; the amplifier is enabled by pulling its SHUTDOWN pin low.
   - Earbuds, wired: a 3.5 mm jack on the speaker output through a resistor attenuator (mono in both ears; the output is bridged, so neither wire goes to ground), or a small I2S DAC board with its own jack if free pins allow.
   - Earbuds, wireless: through the phone page (the phone reads aloud, Bluetooth to the earbuds).
   - Sound: MP3 audiobooks from the card, or speech generated on the Mac, with word timings so the screen follows the voice.
3. **Library, new look** (after the audio add-on): a clearer list showing each book's author, progress, last opened, and whether it has audio or a second language.
4. **Zotero / server library**: browse and download books from the Asus server over Tailscale, convert on the server, send to the reader.
5. Battery reading calibration once a LiPo is connected (GPIO 9 currently reads full scale on USB).
6. Finnegans Wake (public domain in the EU since 2012; keep out of public repos until 2035 for the US).

## Later
- Procedural backgrounds that move slowly (fusing bubbles, merging colours) behind the page.
- Audio pacing via the ES8311 codec (soft tick per word/phrase, lower tone at sentence ends); optional SAM voice; Pink-Trombone-style tract for ambience.
- IMU: face-down to pause; RTC clock in the header; speaker click (ES8311) as optional feedback.
- News sync; games (as in the original project).

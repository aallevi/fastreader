# Plan

## Done
- Reader core: markup (chapters, bold, italic), pacing with adjustable pauses, words per flash, skip amount, chapters, go-to, page view, library with per-book positions, settings record with checksum.
- Interface: 5 themes (Vista, macOS, Scrivener, Windows 98, E-Ink); anti-aliased TrueType text; menu top-left; − speed + then ⏮ ▶ ⏭; rounded/bevelled buttons; guide lines (none / lines / ornate); info only when paused; messages in the title bar; no sliding UI.
- Device (LCDWIKI ES3C28P): display, touch, BOOT button, PWM brightness, rotation, settings + positions saved to flash, octal PSRAM at 80 MHz, books embedded at build time.
- Tools: `tools/convert.py` (EPUB/PDF/TXT → markup), desktop simulator with web view and settings, scripted walkthrough, tests and benchmarks. See [AUDIT.md](AUDIT.md).

## Next
1. **Wi-Fi on the device**: the same web page and settings drawer served by the board (access point first, then home network + `fastreader.local`).
2. **Upload books from the browser** into flash (no SD card needed); then microSD library (4-bit SDMMC on this board).
3. **Zotero / server library**: browse and download books from the Asus server over Tailscale, convert on the server, send to the reader.
4. Battery reading calibration once a LiPo is connected (GPIO 9 currently reads full scale on USB).
5. Finnegans Wake (public domain in the EU since 2012; keep out of public repos until 2035 for the US).

## Later
- Procedural backgrounds that move slowly (fusing bubbles, merging colours) behind the page.
- IMU: face-down to pause; RTC clock in the header; speaker click (ES8311) as optional feedback.
- News sync; games (as in the original project).

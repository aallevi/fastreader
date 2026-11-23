# Credits and bibliography

## Inspiration

- **[AllanBinder/ereader](https://github.com/AllanBinder/ereader)** (Apache-2.0): the
  e-paper speed reader this project started from. Its phrase grouping (up to 3
  words, ending at sentences) and its fixed left edge for phrases, with the
  reasoning behind them, are followed here.
- **Spritz** and the open readers that documented its rule:
  [OpenSpritz](https://github.com/Miserlou/OpenSpritz),
  [Jetzt](https://github.com/ds300/jetzt).

## Hardware

- **LCDWIKI ES3C28P** (ESP32-S3, 2.8" ILI9341V IPS, FT6336 touch, microSD):
  [module page](https://www.lcdwiki.com/2.8inch_ESP32-S3_Display),
  [user manual](https://www.lcdwiki.com/res/ES3C28P/2.8inch_IPS_ESP32-S3_ES3C28P_ES3N28P_User_Manual.pdf).

## Software used

| Part | What | Licence |
|---|---|---|
| Firmware | [esp-hal, esp-radio, esp-rtos, esp-alloc, esp-storage](https://github.com/esp-rs) (Espressif and the esp-rs community) | MIT / Apache-2.0 |
| | [Embassy](https://embassy.dev) executor, net, time, sync | MIT / Apache-2.0 |
| | [edge-dhcp, edge-nal](https://github.com/ivmarkov/edge-net) | MIT / Apache-2.0 |
| | [embedded-sdmmc](https://github.com/rust-embedded-community/embedded-sdmmc-rs), [mipidsi](https://github.com/almindor/mipidsi), [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics), embedded-hal | MIT / Apache-2.0 |
| Rendering | [ab_glyph](https://github.com/alexheretic/ab-glyph) (TrueType outlines), libm, core_maths | Apache-2.0 / MIT |
| Simulator | [minifb](https://github.com/emoon/rust_minifb), [png](https://github.com/image-rs/image-png) | MIT / Apache-2.0 |
| Phone page | [pdf.js](https://github.com/mozilla/pdf.js) (Mozilla), served by the reader | Apache-2.0 |
| Tools | [PyMuPDF](https://pymupdf.readthedocs.io), [fontTools](https://github.com/fonttools/fonttools), NumPy | AGPL-3.0 / MIT / BSD |
| OCR | [LightOnOCR-2-1B (bbox)](https://huggingface.co/lightonai/LightOnOCR-2-1B-bbox-soup) by LightOn, run through [mlx-vlm](https://github.com/Blaizzy/mlx-vlm) via the Scan Studio add-on | Apache-2.0 |
| Testing | [Playwright](https://playwright.dev) (the phone page in WebKit and Chromium) | Apache-2.0 |

Fonts (SIL Open Font License 1.1, subsets): Atkinson Hyperlegible (Braille
Institute), Literata (TypeTogether), IBM Plex (IBM), Crimson Pro, Courier
Prime, Gelasio. See [assets/fonts/LICENSE.md](assets/fonts/LICENSE.md).

## Bibliography

Every entry below was checked against its source on 2026-11-23: articles
against Crossref (DOI metadata) or the publisher, the patent against its
patent-office record, standards against the standard itself.

Reading and RSVP

- Forster, K. I. (1970). Visual perception of rapidly presented word sequences
  of varying complexity. *Perception & Psychophysics*, 8(4), 215–221.
  [doi:10.3758/BF03210208](https://link.springer.com/article/10.3758/BF03210208)
- Potter, M. C. (1984). Rapid serial visual presentation (RSVP): A method for
  studying language processing. In D. E. Kieras & M. A. Just (Eds.), *New
  Methods in Reading Comprehension Research* (pp. 91–118). Hillsdale, NJ: Erlbaum.
- McConkie, G. W., & Rayner, K. (1975). The span of the effective stimulus
  during a fixation in reading. *Perception & Psychophysics*, 17(6), 578–586.
  [doi:10.3758/BF03203972](https://link.springer.com/article/10.3758/bf03203972)
  — about 4 letters left of the fixation and more to the right: the basis
  for the phrase start and the recognition point.
- O'Regan, J. K., & Jacobs, A. M. (1992). Optimal viewing position effect in
  word recognition: A challenge to current theory. *Journal of Experimental
  Psychology: Human Perception and Performance*, 18(1), 185–197.
  [doi:10.1037/0096-1523.18.1.185](https://doi.org/10.1037/0096-1523.18.1.185)
- Rayner, K. (1998). Eye movements in reading and information processing: 20
  years of research. *Psychological Bulletin*, 124(3), 372–422.
  [doi:10.1037/0033-2909.124.3.372](https://doi.org/10.1037/0033-2909.124.3.372)
- Rayner, K., Schotter, E. R., Masson, M. E. J., Potter, M. C., & Treiman, R.
  (2016). So much to read, so little time: How do we read, and can speed
  reading help? *Psychological Science in the Public Interest*, 17(1), 4–34.
  [doi:10.1177/1529100615625998](https://journals.sagepub.com/doi/10.1177/1529100615625998)
  — a sober review of what speed reading can and cannot do.
- Waldman, F. A., Maurer, M. S., & Skrypnyuk, O. (2016). *Methods and systems
  for displaying text using RSVP*. US Patent 9,483,109 B2, granted 1 November
  2016, assigned to Spritz Technology Inc.
  [Google Patents](https://patents.google.com/patent/US9483109B2/en)
  — the recognition-letter rule by word length and splitting words over 13
  letters.

Interface

- Fitts, P. M. (1954). The information capacity of the human motor system in
  controlling the amplitude of movement. *Journal of Experimental Psychology*,
  47(6), 381–391. [doi:10.1037/h0055392](https://doi.org/10.1037/h0055392)
  — touch areas split between buttons.
- Bayer, B. E. (1973). An optimum method for two-level rendition of
  continuous-tone pictures. *Conference Record of the IEEE International
  Conference on Communications*, 26-11–26-15. — the ordered dithering to the
  panel's 16-bit colour.

Standards

- W3C (2010). *Speech Synthesis Markup Language (SSML) Version 1.1*. W3C
  Recommendation, 7 September 2010.
  [w3.org](https://www.w3.org/TR/2010/REC-speech-synthesis11-20100907/) — the
  model for the pause marks (`{pause}`, `{slow}`).
- Kumari, W., & Kline, E. (2020). *Captive-Portal Identification in DHCP and
  Router Advertisements (RAs)*. RFC 8910.
  [rfc-editor.org](https://www.rfc-editor.org/rfc/rfc8910) — the phone opening
  the reader's page.
- Microsoft (2000). *FAT: General Overview of On-Disk Format*, version 1.03,
  6 December 2000. — the SD card formatter.
- W3C. *EPUB 3*. [w3.org/TR/epub-33](https://www.w3.org/TR/epub-33/) — the
  books the converters read and `ocr_book.py` writes.

OCR

- Taghadouini, S., Cavaillès, A., & Aubertin, B. (2026). LightOnOCR: A 1B
  end-to-end multilingual vision-language model for state-of-the-art OCR.
  [arXiv:2601.14251](https://arxiv.org/abs/2601.14251).

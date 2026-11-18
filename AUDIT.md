# Function audit

Every function checked against established usability rules and RSVP reading research. A ✔ marks something verified by an automated test or an on-device measurement; the test or measurement is named next to it.

## 1. Usability rules (Nielsen's 10 heuristics + Shneiderman's 8 golden rules)

| Rule | How the reader meets it | Evidence |
|---|---|---|
| Visibility of system status | Every key/tap gives feedback: its button lights (160 ms) and a message replaces the title for 1.4 s. Info (%, word n of N, time left, chapter) appears whenever paused. | ✔ `every_key_on_every_screen_gives_feedback`, ✔ `every_key_shows_feedback_in_the_title_bar_or_by_lighting_its_button` |
| Match with the real world | Music-player transport ⏮ ▶ ⏭; speed as − 300 +; page view like a book. | design |
| User control and freedom | ◀ / Back always goes back one level, Menu always closes everything, browsing a list (theme, font…) previews live and Back restores what you had. | ✔ `same_key_same_meaning_everywhere`, ✔ `theme_preview_and_cancel_restores` |
| Consistency and standards | One binding table per screen; on-screen buttons, physical keys and the web page all go through it. Same key = same meaning on every screen. | ✔ `taps_equal_keys`, ✔ `every_key_has_an_on_screen_button_while_reading` |
| Error prevention | No hidden gestures, no swipes: only drawn buttons. A tap acts on release; sliding off cancels. Settings are clamped; unknown names refused. | ✔ `sliding_off_cancels`, ✔ `set_clamps_and_refuses_unknown`, ✔ web `/set?name=evil` → 400 |
| Recognition rather than recall | Menu rows show current values; lists mark what was set before (radio dot); font names are drawn in their own font. | walkthrough frames |
| Flexibility and efficiency | Hold ◀◀/▶▶ to scroll word by word; hold −/+ to repeat; skip amount configurable; browser settings for everything. | ✔ `holding_back_scrolls_then_release_resumes`, ✔ `skip_setting_changes_what_the_skip_buttons_do` |
| Aesthetic and minimalist design | While playing: only the word, guides and a hairline progress bar. Text appears only when paused. | ✔ walkthrough |
| Help users recognise errors | Limits are amber and say why ("Fastest: 1000 wpm", "Start of book", "This book has no chapters"). Nothing fails silently. | ✔ `limits_are_reported_not_silent`, ✔ `no_chapters_is_explained` |
| Closure of dialogs | Choosing in a list returns to the menu with a confirmation; jumps (chapter, go to) return to reading. | ✔ `chapters_jump_and_return_to_reading` |
| Reversal of actions | Positions remembered per book; "Start over" is a menu item, not a gesture. | ✔ `library_switches_books_and_remembers_positions` |

## 2. Touch targets (Fitts' law; Apple HIG 44 pt, Material 48 dp ≈ 7–9 mm)

The 2.8" 320×240 panel is ~143 ppi, so 1 px ≈ 0.18 mm. Every toolbar hit area spans the full 48 px (8.5 mm) bar height and at least 34 px (6 mm) width, with the space between buttons split halfway so nothing is dead. ✔ `toolbar_hit_areas_never_overlap_and_cover_their_buttons` checks 320×240, 240×320 and 480×320. Menu rows are 29 px × full width.

## 3. RSVP reading research

| Known finding | What the reader does |
|---|---|
| RSVP removes regressions (glancing back), which hurts comprehension (Schotter et al. 2014) | Sentence shown under the word when paused; hold ◀◀ to scroll back as a line; ◀◀ jumps by your chosen amount. |
| Punctuation and clause boundaries need extra time | Comma ×1.5, sentence ×2.2, paragraph ×3.0 (all adjustable from the browser). ✔ `classifies_pauses` |
| Long words take longer to recognise | +8% per letter past 8, capped (can be turned off). ✔ `long_words_linger_but_capped` |
| A start-up ramp helps the eye lock on | First 4 words slower after play (can be turned off). ✔ `ramp_can_be_turned_off` |
| Chunking 2–3 words can help at high speed, but never across phrases | Words per flash 1–3; groups stop at punctuation and headings. ✔ `groups_stop_at_punctuation_and_headings` |
| Optimal recognition point (red letter) | Removed at your request; words are centred between guide lines with centre markers. |
| Timing must not drift | Schedule advances from the planned time, never bursts after a late wake-up. ✔ `coarse_ticks_do_not_drift`, ✔ `late_wakeup_skips_no_words` |

## 4. Efficiency (measured on the ES3C28P)

| | Result |
|---|---|
| Word redraw | 6–8 ms canvas + only the old/new word box sent over SPI (✔ `word_change_repaints_only_the_word`: ≤40% of the screen) |
| Idle | nothing redrawn, no wake-ups (✔ `idle_frames_cost_nothing`) |
| Open a 166k-word book | 1.05 s, 5.5 MB RAM still free |
| Page view | opens at once; the rest of the book is paginated a few pages per frame |
| Fonts | 28 anti-aliased faces, 1.1 MB flash, 5 KB RAM |

## 5. Compared with the original project (AllanBinder/ereader)

| Original feature | Here |
|---|---|
| Normal page reading | ✔ Page view (menu → View), ◀ ▶ turn pages, ⚡ speed-reads from that page |
| Text size Normal/Large | ✔ Small/Normal/Large, live preview |
| Position memory across power cycles | ✔ per book |
| Jump to page | ✔ Go to (0–90%) and Chapters list (original had no chapter navigation) |
| Speed read 100–600+ wpm, +25 per press | ✔ 100–1000, +25, hold to repeat, presets list |
| 1/2/3 words per flash | ✔ |
| Step back 20 words | ✔ ◀◀ jumps by 1 sentence / 5 / 10 / 25 / 50 words / 1 paragraph (your choice) |
| Flip screen | ✔ Rotate screen |
| Button test | ✔ Device info (board, firmware, touch chip, battery, memory) |
| Books: upload .txt / converted PDFs | ✔ `tools/convert.py` for EPUB/PDF/TXT keeping chapters, bold, italic; books embedded in flash. Upload over Wi-Fi: planned |
| Wi-Fi web interface, mDNS, AP setup | Browser view + full settings work in the simulator; **on the device: next step** |
| News sync, metrics | planned |
| NES / Game Boy emulators, Pokémon dashboard | planned last |
| BME280 sensor in menu header | not applicable (this board has an IMU and RTC instead: planned) |
| E-paper ghosting workarounds (turbo waveforms) | not needed: LCD |

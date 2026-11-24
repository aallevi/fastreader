# Verification checklist

Firmware flashed on the reader. Tick each line; note anything odd next to it.

## Settings
- [ ] After this update the reader kept your settings (500 wpm, Windows 98, brightness 1, recognition point, brackets as side marks). Change one, restart: it stays.

## Reading
- [ ] Word mode: words **centred** (Aa → Word position → Centred); size never changes between words.
- [ ] Aa → Word position → Recognition point: words shift so the eye stays on one spot, slightly left of centre.
- [ ] Phrase mode: groups of up to 3 words, same size every flash, end at sentence ends.
- [ ] Ticker: the line jumps one word at a time like a typewriter (no gliding).
- [ ] Page view: ◀ ▶ turn whole pages like a book; holding repeats; the phone's web view follows.
- [ ] ⏮ while playing: one press goes back to the sentence start; a second press within 2.5 s goes one sentence further.
- [ ] Chapter titles bold and on screen twice as long; bold/italic kept.

## Pictures, long words, the sentence under the word
- [ ] Aa → Picture time: 2, 3, 4, 8, 15 s or until play; try 3 s and "Until I press play"; the time starts once the picture is on screen (a picture from the card may take a moment).
- [ ] Aa → Long words: "Keep whole" (default) shows long words whole, shifted to fit; "Split in two if too long" shows e.g. "contempo-" then "raneamente" (both halves appear); Italian elisions ("c'è", "un'opera") focus on the letter, not the apostrophe.
- [ ] Paused on a long sentence: ▲▼ appear beside the sentence and scroll it; at the ends they dim and tapping shows top/bottom.
- [ ] The sentence under the word runs out to the screen edges (short sentences stay centred).
- [ ] Gombrich on the card, while playing: each picture appears whole at once (no lower half missing, nothing left behind under the next words) and stays 4 s.
- [ ] Paused on a picture with a description: a **Description** button at its bottom right shows the caption over the picture; **Hide** removes it; the choice holds for the next pictures. (Captions come with the OCR EPUB; the PDF version on the card has none, so no button there.)

## Brackets
- [ ] Cancroregina, ~53% ("Pomposo esordio … fatterello …"): with Aa → Brackets → As side marks, "fatterello" still shows the ( ) marks.
- [ ] [ ], { }, « », < >, << >> act the same as ( ).
- [ ] A short pause as an aside opens and closes; asides go at normal speed.

## Bookmarks
- [ ] Tap the page's top-right corner: it folds down (shadow + crease), "Bookmarked" appears. Tap again: removed.
- [ ] 📖 → Bookmarks lists them with % and first words; tapping one jumps there.
- [ ] Switch off (⚙ → Sleep), wake with BOOT: same book, same place, bookmarks kept.
- [ ] Windows 98 theme: corner drawn in pixels (teal cut, grey bevelled flap, dotted shadow).

## Menus and layout
- [ ] Title bar: ⚙ Settings, 📖 Book, 📚 Library, Aa Text open their menus; touch areas reach a little below the bar.
- [ ] 📚 opens the book list directly; ◀ returns to reading.
- [ ] BOOT long press opens 📖; ▲▼ walk through all menus; ◀ backs out.
- [ ] Text area runs edge to edge in every theme (no grey side bands).

## Pictures
- [ ] A book with figures: each picture fills the stage for ~4 words' time; in page view it sits in the text; the web view shows it.

## SD card
- [ ] ⚙ Settings → SD card shows "Needs formatting" (your card is exFAT). Choose it → **Erase and format** → "SD card ready", then it shows the free space (~14.8 GB).
- [ ] Gombrich from the phone: the phone warns it's too heavy (scanned, 702 pages) and suggests a computer.
- [ ] Gombrich via the computer: card in the Mac, `tools/convert.py gombrich-storia-arte.pdf --sd /Volumes/FASTREADER`, eject, card back in the reader, restart. It appears in 📚; pictures (422) show as you read and in page view.
- [ ] A normal PDF or EPUB from the phone goes to the card.
- [ ] Switch off and on: the card books are still in 📚.

## Deleting books on the reader
- [ ] 📖 Book → Delete a book… → pick one → "Keep it" is chosen first; ◀ goes back to the list; "Delete «title»" removes it and the Library updates.

## Bottom of the screen
- [ ] ⚙ Settings → Progress bar / Book info when paused: each can be hidden; the space goes back to the page.
- [ ] Windows 98 progress bar: whole blocks appear one at a time as you read (no block growing); ⚙ Settings → Progress blocks → Blue gives the classic navy.

## Menu buttons
- [ ] In any menu the four title-bar buttons stay; the open one is pressed in (amber); tapping another switches menu; ‹ and ✕ sit on the right, a little apart.
- [ ] Settings > Screen > Menu buttons > Colourful: red ⚙, blue 📖, green 📚, yellow Aa with white icons; the open one darker, pressed in.

## Lists and tables
- [ ] A book with a bulleted list: bullets down the left margin, the current one amber; a numbered list shows its numbers.
- [ ] A table: before each cell a quick grid with the cell in amber, then its words; header row shaded.

## Read aloud (phone)
- [ ] Phone page > 🔊 Read aloud: the phone reads from the reader's place; the reader shows each word as it is spoken; ■ Stop reading leaves the reader paused there. Try Italian and English books (the voice follows the language).

## Menu levels
- [ ] ⚙ shows Reading ▸, Screen ▸, Connections ▸, Device info, Sleep; each heading opens its own list with its name on top; ‹ goes back up, then closes.
- [ ] Aa shows Reading mode, Text size, Font, Layout ▸, Content ▸.
- [ ] Choosing a setting inside a submenu comes back to that submenu.

## Home Wi-Fi
- [ ] Web page > Home Wi-Fi: type your network and password, Save and join. The reader restarts; Device info shows "http://fastreader.local (address)".
- [ ] From the Mac on the home network: open http://fastreader.local; the FastReader network still works too.
- [ ] Wrong password: Device info says "wrong password?" and keeps retrying; Forget restarts without it.

## Welcome back
- [ ] Switch the reader on: the card shows the book, author, chapter and %; ▶ restarts from the start of the sentence; any other touch just closes it.
- [ ] Leave it paused for the set time (⚙ → Welcome back): the card appears with "Last read … ago".
- [ ] 📖 Book: the first line is "Reading | title  author", on one line inside its frame; tapping it resumes reading.
- [ ] A bookmarked word next to a bookmarked picture: the folded corner stays on top of the picture.
- [ ] Mac: open http://192.168.4.1 in Safari, drag a book onto the page.

## Guide lines
- [ ] Aa → Guide lines → On: a line above and below the word across the page, with a tick at the centre letter (a groove in Windows 98, thin grey elsewhere); gone while a picture shows, back with the next word; word and phrase modes.

## Jump counts
- [ ] Press ⏭ four times and ⏮ once quickly: the message counts 1, 2, 3, 4 and ends on "3 sentences forward"; after it disappears the count starts again. The same for page turns and the thin ‹ ›.

## Word steps
- [ ] The thin ‹ › beside play move exactly one word and pause (reading and page view): use them to place the highlight or a bookmark.

## Deleting books (web view → Settings → Books on the reader)
- [ ] Each book shows where it is (SD card / reader's memory) and its size; Delete asks once more and the question disappears by itself after a few seconds.
- [ ] More… → delete everything in one place asks you to type DELETE (anything else deletes nothing).

## Web view (⚙ → Web view, join "FastReader")
- [ ] The phone opens the page by itself on joining (captive portal); otherwise http://192.168.4.1.
- [ ] Settings drawer changes the reader live (incl. Word position, Parentheses).
- [ ] Add a book (EPUB with pictures) → it appears in 📚 with its pictures.
- [ ] Add a **PDF** from the phone: progress shows page by page; the book appears in 📚 with chapters, bold/italic and pictures; no page numbers or running heads in the text.
- [ ] Bookmarked words show a small dog-ear in the web text.

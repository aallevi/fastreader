# Verification checklist

Firmware flashed on the reader. Tick each line; note anything odd next to it.

## Reading
- [ ] Word mode: words **centred** (Aa → Word position → Centred); size never changes between words.
- [ ] Aa → Word position → Recognition point: words shift so the eye stays on one spot, slightly left of centre.
- [ ] Phrase mode: groups of up to 3 words, same size every flash, end at sentence ends.
- [ ] Ticker: the line jumps one word at a time like a typewriter (no gliding).
- [ ] Page view: ◀ ▶ turn whole pages like a book; holding repeats; the phone's web view follows.
- [ ] ⏮ while playing: one press goes back to the sentence start; a second press within 2.5 s goes one sentence further.
- [ ] Chapter titles bold and on screen twice as long; bold/italic kept.

## Pictures, long words, the sentence under the word
- [ ] Aa → Picture time: try 4 s and "Until I press play"; the time starts once the picture is on screen (a picture from the card may take a moment).
- [ ] Aa → Word position → Recognition point: a long word ("contemporaneamente") comes in two flashes, the eye point never moves; Italian elisions ("c'è", "un'opera") focus on the letter, not the apostrophe.
- [ ] Paused on a long sentence: ▲▼ appear beside the sentence and scroll it; at the ends they dim and tapping shows top/bottom.

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

## Web view (⚙ → Web view, join "FastReader")
- [ ] The phone opens the page by itself on joining (captive portal); otherwise http://192.168.4.1.
- [ ] Settings drawer changes the reader live (incl. Word position, Parentheses).
- [ ] Add a book (EPUB with pictures) → it appears in 📚 with its pictures.
- [ ] Add a **PDF** from the phone: progress shows page by page; the book appears in 📚 with chapters, bold/italic and pictures; no page numbers or running heads in the text.
- [ ] Bookmarked words show a small dog-ear in the web text.

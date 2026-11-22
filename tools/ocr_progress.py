"""A loading bar for a running ocr_book.py: reads its log and redraws.

    python3 tools/ocr_progress.py books/private/gombrich-ocr.log

Ctrl-C to stop watching (the OCR keeps running)."""

import re
import sys
import time


def bar(done, total, per, width=40):
    frac = done / total if total else 0
    fill = int(frac * width)
    left = (total - done) * per
    h, m = divmod(int(left // 60), 60)
    eta = f"{h} h {m:02d} min" if h else f"{m} min"
    return f"\r[{'█' * fill}{'░' * (width - fill)}] {frac * 100:5.1f}%  page {done}/{total}  {per:.0f} s/page  ~{eta} left "


def main():
    log = sys.argv[1] if len(sys.argv) > 1 else "books/private/gombrich-ocr.log"
    line = re.compile(r"page (\d+): ([\d.]+) s/page, (\d+) to go")
    try:
        while True:
            text = open(log, encoding="utf-8", errors="replace").read()
            found = line.findall(text)
            if found:
                n, per, left = found[-1]
                left = int(left)
                done = len(found)  # one log line per page read in this run
                total = done + left
                sys.stdout.write(bar(done, total, float(per)))
                sys.stdout.flush()
            last = text.strip().splitlines()[-1] if text.strip() else ""
            if "illustrations" in last:  # ocr_book.py's final line: the EPUB is written
                print("\n" + last)
                return
            time.sleep(2)
    except KeyboardInterrupt:
        print()


if __name__ == "__main__":
    main()

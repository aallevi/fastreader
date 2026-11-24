"""Convert EPUB / PDF / TXT into the reader's markup.

    tools/.venv/bin/python tools/convert.py book.epub [more files...] [--out books/private]

Output is plain text with a light Markdown subset (what reader-core reads):
  # Chapter title        headings (stay on screen longer, shown bold)
  **bold**  *italic*     kept from the source fonts / tags
  blank line             paragraph break

PDF: chapters come from the table of contents when there is one, otherwise
from text noticeably larger than the body size. Running headers, footers
and page numbers are dropped, hyphenated line ends are rejoined.
EPUB: follows the spine order; h1–h4 are headings; b/strong and i/em (and
CSS classes that set bold/italic) are kept. Images, scripts and note
markers are skipped.
"""
import argparse
import collections
import html
import posixpath
import re
import struct
import sys
import unicodedata
import zipfile
from html.parser import HTMLParser
from pathlib import Path
from xml.etree import ElementTree as ET


def clean(s: str) -> str:
    # Markers are ours: drop stray * and _ from the source, and soft hyphens.
    s = s.replace("­", "").replace("*", "").replace("_", " ")
    s = unicodedata.normalize("NFC", s)
    return re.sub(r"[ \t ]+", " ", s)


def wrap(text: str, bold: bool, italic: bool) -> str:
    """Wraps each word, so a style never spans a line we might split."""
    if not (bold or italic) or not text.strip():
        return text
    m = "***" if bold and italic else "**" if bold else "*"
    lead = text[: len(text) - len(text.lstrip())]
    trail = text[len(text.rstrip()):]
    return lead + " ".join(m + w + m for w in text.split()) + trail


# ---------------------------------------------------------------- EPUB

class XhtmlText(HTMLParser):
    BLOCK = {"p", "div", "li", "blockquote", "br", "tr", "section", "article", "dd", "dt"}
    HEAD = {"h1", "h2", "h3", "h4"}
    SKIP = {"script", "style", "head", "sup", "svg", "title"}

    def __init__(self, bold_classes, italic_classes, picture=None):
        super().__init__(convert_charrefs=True)
        self.out = []
        self.line = []
        self.stack = []
        self.skip = 0
        self.heading = False
        self.bold_classes, self.italic_classes = bold_classes, italic_classes
        # picture(src) -> "{image:N}" or "" for an <img>/<image>.
        self.picture = picture
        # Lists: [ordered, next number] per open <ul>/<ol>; the marker for
        # the <li> being read. Tables: rows of cells, read as | a | b | lines.
        self.lists = []
        self.marker = ""
        self.table = None  # (rows, header?) while inside a <table>
        self.cell = None
        # Verse (a poem or song in a <blockquote>): its lines, kept as lines.
        self.verse = None
        self.verse_depth = 0

    def style(self):
        b = any(t in ("b", "strong") or c & self.bold_classes for t, c in self.stack)
        i = any(t in ("i", "em", "cite") or c & self.italic_classes for t, c in self.stack)
        return b, i

    def flush(self):
        text = re.sub(r"\s+", " ", "".join(self.line)).strip()
        self.line = []
        if self.verse is not None:
            if text:
                self.verse.append(text)
            return
        if self.cell is not None:
            # Inside a table cell: the text belongs to the cell.
            if text:
                self.cell.append(text)
            return
        if text:
            if self.heading:
                self.out.append("# " + re.sub(r"\*+", "", text))
            elif self.marker:
                self.out.append(self.marker + " " + text)
                self.marker = ""
            else:
                self.out.append(text)

    def handle_starttag(self, tag, attrs):
        classes = set((dict(attrs).get("class") or "").split())
        if tag == "figcaption" and not self.skip:
            # A picture's description: kept with the picture, not read as text.
            self.flush()
            self.caption = True
        if tag in ("img", "image") and self.picture and not self.skip:
            a = dict(attrs)
            mark = self.picture(a.get("src") or a.get("xlink:href") or a.get("href") or "")
            if mark:
                self.flush()
                self.out.append(mark)
            return
        if tag in self.SKIP:
            self.skip += tag not in ("br", "img")
            return
        if tag == "blockquote" and self.verse is None:
            kind = (dict(attrs).get("class") or "") + " " + (dict(attrs).get("epub:type") or "")
            if re.search(r"poem|song|verse|hymn|lyric", kind):
                self.flush()
                self.verse = []
                self.verse_depth = len(self.stack)
        if tag in ("ul", "ol"):
            self.flush()
            start = dict(attrs).get("start") or "1"
            self.lists.append([tag == "ol", int(start) if start.isdigit() else 1])
        if tag == "li":
            self.flush()
            # Nested lists: two spaces of indentation per level.
            pad = "  " * max(0, len(self.lists) - 1)
            if self.lists and self.lists[-1][0]:
                self.marker = f"{pad}{self.lists[-1][1]}."
                self.lists[-1][1] += 1
            else:
                self.marker = pad + "•"
        if tag == "table":
            self.flush()
            self.table = ([], False)
        if tag in ("td", "th") and self.table is not None:
            self.flush()
            self.cell = []
            if tag == "th" and len(self.table[0]) <= 1:
                self.table = (self.table[0], True)
        if tag == "tr" and self.table is not None:
            self.table[0].append([])
        if tag in self.BLOCK or tag in self.HEAD:
            self.flush()
            if tag in self.HEAD:
                self.heading = True
        if tag != "br":
            self.stack.append((tag, classes))

    def handle_endtag(self, tag):
        if tag == "figcaption" and getattr(self, "caption", False):
            text = re.sub(r"\s+", " ", "".join(self.line)).strip()
            self.line = []
            self.caption = False
            if text and self.out and self.out[-1].startswith("{image:"):
                self.out.append("{caption} " + text + " {/caption}")
            elif text:
                self.out.append(text)
            return
        if tag in self.SKIP:
            self.skip = max(0, self.skip - (tag not in ("br", "img")))
            return
        if tag == "blockquote" and self.verse is not None and len(self.stack) - 1 <= self.verse_depth:
            self.flush()
            lines, self.verse = self.verse, None
            if lines:
                self.out.append("{verse}\n" + "\n".join(lines) + "\n{/verse}")
        if tag in ("ul", "ol") and self.lists:
            self.flush()
            self.lists.pop()
        if tag in ("td", "th") and self.cell is not None:
            self.flush()
            text = " ".join(self.cell).replace("|", "/")
            self.cell = None
            if self.table is not None:
                if not self.table[0]:
                    self.table[0].append([])
                self.table[0][-1].append(text)
        if tag == "table" and self.table is not None:
            rows, header = self.table
            self.table = None
            rows = [r for r in rows if any(c.strip() for c in r)]
            if rows:
                width = max(len(r) for r in rows)
                for k, r in enumerate(rows):
                    r = r + [""] * (width - len(r))
                    self.out.append("| " + " | ".join(r) + " |")
                    if k == 0 and header:
                        self.out.append("|" + "---|" * width)
        for i in range(len(self.stack) - 1, -1, -1):
            if self.stack[i][0] == tag:
                del self.stack[i:]
                break
        if tag in self.BLOCK or tag in self.HEAD:
            self.flush()
            self.heading = False

    def handle_data(self, data):
        if self.skip:
            return
        b, i = self.style()
        self.line.append(wrap(clean(data), b, i))


# ---------------------------------------------------------------- images
# Pictures go after the text as RGB565, scaled to fit the reader's stage.
# The text refers to them as {image:N}. A budget keeps books small enough
# to upload and to hold in the reader's memory.

IMG_W, IMG_H = 300, 190
IMG_BUDGET = 2_000_000  # bytes of pixels per book (reader's flash, no SD card)
CARD_BUDGET = 200_000_000  # writing to the SD card: keep every picture
IMG_MIN = 48  # smaller pictures are decoration (rules, bullets, icons)
# Version 2: a table of (width, height, offset) first, so the reader can
# fetch one picture from the card without reading the others.
IMAGES_MAGIC2 = b"\n\0FRIMG2"


class Images:
    def __init__(self, budget=IMG_BUDGET):
        self.items = []
        self.used = 0
        self.budget = budget

    def add_pixmap(self, pm):
        """Adds a pymupdf Pixmap; returns the {image:N} mark or ""."""
        import pymupdf
        if pm.width < IMG_MIN or pm.height < IMG_MIN:
            return ""
        if pm.n - pm.alpha != 3:
            pm = pymupdf.Pixmap(pymupdf.csRGB, pm)
        if pm.alpha:
            # See-through pictures (line drawings on transparent) go on white
            # paper; dropping the alpha alone would leave them black.
            import numpy as np
            a = np.frombuffer(pm.samples, np.uint8).reshape(pm.height, pm.stride)[:, : pm.width * pm.n].reshape(pm.height, pm.width, pm.n).astype(np.uint16)
            rgb, al = a[:, :, :3], a[:, :, 3:4]
            flat = ((rgb * al + 255 * (255 - al)) // 255).astype(np.uint8)
            pm = pymupdf.Pixmap(pymupdf.csRGB, pm.width, pm.height, flat.tobytes(), False)
        scale = min(IMG_W / pm.width, IMG_H / pm.height, 1.0)
        w, h = max(1, int(pm.width * scale)), max(1, int(pm.height * scale))
        if self.used + w * h * 2 > self.budget:
            return ""
        import numpy as np
        a = np.frombuffer(pm.samples, np.uint8).reshape(pm.height, pm.stride)[:, : pm.width * pm.n].reshape(pm.height, pm.width, pm.n)[:, :, :3]
        # Nearly all white: an empty stretch of page, not a picture.
        if (a.astype(np.uint16).sum(axis=2) < 600)[::4, ::4].mean() < 0.03:
            return ""
        # Area-average downscale to the reader's size.
        ys = (np.arange(h + 1) * pm.height // h)
        xs = (np.arange(w + 1) * pm.width // w)
        c = np.add.reduceat(np.add.reduceat(a.astype(np.uint32), ys[:-1], axis=0), xs[:-1], axis=1)
        c = (c / ((ys[1:] - ys[:-1])[:, None, None] * (xs[1:] - xs[:-1])[None, :, None])).astype(np.uint16)
        v = (c[:, :, 0] >> 3) << 11 | (c[:, :, 1] >> 2) << 5 | c[:, :, 2] >> 3
        out = v.astype("<u2").tobytes()
        self.items.append((w, h, out))
        self.used += w * h * 2
        return f"{{image:{len(self.items) - 1}}}"

    def add_bytes(self, data: bytes):
        import pymupdf
        try:
            return self.add_pixmap(pymupdf.Pixmap(data))
        except Exception:
            return ""

    def section(self) -> bytes:
        if not self.items:
            return b""
        n = len(self.items)
        out = bytearray(IMAGES_MAGIC2) + struct.pack("<HH", n, 0)
        off = 4 + 8 * n
        for w, h, px in self.items:
            out += struct.pack("<HHI", w, h, off)
            off += len(px)
        for _, _, px in self.items:
            out += px
        return bytes(out)


def css_classes(css: str):
    bold, italic = set(), set()
    for sel, body in re.findall(r"([^{}]+)\{([^}]*)\}", css):
        # Only plain ".name" or "tag.name" selectors: a class styled in one
        # particular spot ("h2 + p.first-child") isn't bold everywhere.
        names = {m.group(1) for part in sel.split(",") if (m := re.fullmatch(r"\s*[\w-]*\.([\w-]+)\s*", part))}
        if re.search(r"font-weight\s*:\s*(bold|[6-9]00)", body):
            bold |= names
        if re.search(r"font-style\s*:\s*(italic|oblique)", body):
            italic |= names
    return bold, italic


def epub(path: Path, images: Images):
    z = zipfile.ZipFile(path)
    ns = {"c": "urn:oasis:names:tc:opendocument:xmlns:container", "o": "http://www.idpf.org/2007/opf", "dc": "http://purl.org/dc/elements/1.1/"}
    opf_path = ET.fromstring(z.read("META-INF/container.xml")).find(".//c:rootfile", ns).get("full-path")
    opf = ET.fromstring(z.read(opf_path))
    base = posixpath.dirname(opf_path)
    title_el = opf.find(".//dc:title", ns)
    title = title_el.text.strip() if title_el is not None and title_el.text else path.stem
    creator = opf.find(".//dc:creator", ns)
    images.author = creator.text.strip() if creator is not None and creator.text else ""
    items = {i.get("id"): i.get("href") for i in opf.find("o:manifest", ns)}
    bold, italic = set(), set()
    for href in items.values():
        if href.endswith(".css"):
            b, i = css_classes(z.read(posixpath.join(base, href)).decode("utf-8", "replace"))
            bold |= b
            italic |= i
    parts = []
    for ref in opf.find("o:spine", ns):
        href = items.get(ref.get("idref"), "")
        if not href.endswith((".xhtml", ".html", ".htm")):
            continue
        doc = posixpath.join(base, html.unescape(href))

        def picture(src, doc=doc):
            name = posixpath.normpath(posixpath.join(posixpath.dirname(doc), html.unescape(src).split("#")[0]))
            try:
                return images.add_bytes(z.read(name))
            except KeyError:
                return ""
        source = z.read(doc).decode("utf-8", "replace")
        # A publisher's pages, not the book: imprint, colophon, copyright,
        # list of illustrations, title pages (marked so in EPUB 3).
        if re.search(r'<(?:body|section|nav)[^>]*epub:type="[^"]*\b(imprint|colophon|copyright-page|loi|halftitlepage|titlepage)\b', source):
            continue
        p = XhtmlText(bold, italic, picture)
        p.feed(source)
        p.flush()
        parts.extend(p.out)
    return title, "\n\n".join(parts)


# ---------------------------------------------------------------- PDF

def pdf(path: Path, images: Images):
    import pymupdf

    d = pymupdf.open(path)
    title = (d.metadata or {}).get("title") or path.stem.split(" -- ")[0]
    images.author = ((d.metadata or {}).get("author") or "").strip()
    # Body size = the size most characters are set in.
    sizes = collections.Counter()
    lines_by_page = []
    for pg in d:
        page_lines = []
        pictures = []
        area = pg.rect.width * pg.rect.height
        # Where images sit (no decoding needed); each is cut from the
        # rendered page, which also applies masks and clipping.
        for info in pg.get_image_info():
            r = pymupdf.Rect(info["bbox"]) & pg.rect
            if r.width > pg.rect.width * 0.15 and r.height > pg.rect.height * 0.08:
                pictures.append((tuple(r), ("drawing", r), r.width * r.height > area * 0.6))
        for b in pg.get_text("dict", flags=pymupdf.TEXT_PRESERVE_WHITESPACE)["blocks"]:
            if b.get("type") != 0:
                continue
            for ln in b.get("lines", []):
                spans = [s for s in ln["spans"] if s["text"].strip()]
                if not spans or not re.search(r"\w", "".join(s["text"] for s in spans)):
                    continue  # empty, or pure symbols (OCR noise in pictures)
                for s in spans:
                    sizes[round(s["size"])] += len(s["text"])
                page_lines.append((b["number"], ln["bbox"], spans))
        # A scanned book: each page is one big photo with the OCR text on
        # top. That photo is the page itself: keep the text; illustrations
        # are the tall stretches without text lines, cut from the page.
        if page_lines and any(big for _, _, big in pictures):
            pictures = [p for p in pictures if not p[2]]
            h = pg.rect.height
            top = h * 0.06
            for y0, y1 in sorted((ln[1][1], ln[1][3]) for ln in page_lines) + [(h * 0.94, h)]:
                if y0 - top > h * 0.2:
                    r = pymupdf.Rect(pg.rect.width * 0.06, top, pg.rect.width * 0.94, y0)
                    pictures.append((tuple(r), ("drawing", r), False))
                top = max(top, y1)
        elif not page_lines and any(big for _, _, big in pictures):
            # A page without text in a scanned book: a plate or the cover.
            pictures = [(tuple(pg.rect), ("drawing", pg.rect), False)]
        pictures = [(bbox, pic) for bbox, pic, _ in pictures]
        # Charts and diagrams drawn as vectors: render each cluster as a picture.
        try:
            clusters = pg.cluster_drawings()
        except Exception:
            clusters = []
        for r in clusters if len(clusters) < 40 else []:
            if r.width > pg.rect.width * 0.2 and r.height > pg.rect.height * 0.08:
                pictures.append((tuple(r), ("drawing", r)))
        # Tables: read as the reader's | a | b | rows, in place of their loose
        # lines of text (and of the ruled lines that would come out as a
        # "drawing"). The first row is taken as the header.
        try:
            found = pg.find_tables().tables
        except Exception:
            found = []
        page_words = pg.get_text("words") if found else []

        def cell_text(box):
            # The words whose middle lies in the cell, in reading order (the
            # library's own extract() glues a cell's lines without a space).
            if not box:
                return ""
            r = pymupdf.Rect(box)
            ws = [w for w in page_words if r.contains(pymupdf.Point((w[0] + w[2]) / 2, (w[1] + w[3]) / 2))]
            ws.sort(key=lambda w: (round(w[1] / 3), w[0]))
            text = " ".join(w[4] for w in ws).replace("|", "/")
            return re.sub(r"\s+", " ", re.sub(r"[■□▪◆●]", " ", text)).strip()

        for t in found:
            rows = [[cell_text(c) for c in row.cells] for row in t.rows]
            rows = [r for r in rows if any(r)]
            if len(rows) < 2 or max(len(r) for r in rows) < 2:
                continue
            # A contents page laid out as a grid (titles, then page numbers):
            # leave it as text.
            pagey = sum(any(re.fullmatch(r"[\divxlcIVXLC ]{1,12}", c) for c in r[1:]) for r in rows)
            if len(rows) >= 5 and pagey >= len(rows) * 0.6:
                continue
            tb = pymupdf.Rect(t.bbox)
            inside = lambda b: tb.contains(pymupdf.Point((b[0] + b[2]) / 2, (b[1] + b[3]) / 2))
            page_lines = [e for e in page_lines if isinstance(e[2], tuple) or not inside(e[1])]
            pictures = [(b, pic) for b, pic in pictures if (pymupdf.Rect(b) & tb).get_area() < 0.5 * pymupdf.Rect(b).get_area()]
            pictures.append((tuple(tb), ("table", rows)))
        # Each picture goes before the first text line below its top; text
        # keeps its own (column-aware) order.
        for bbox, pic in sorted(pictures, key=lambda e: e[0][1]):
            at = next((k for k, e in enumerate(page_lines) if not isinstance(e[2], tuple) and e[1][1] >= bbox[1]), len(page_lines))
            page_lines.insert(at, (-1, bbox, pic))
        lines_by_page.append((pg.rect.height, page_lines, pg))
    body = sizes.most_common(1)[0][0] if sizes else 11

    # Lines that repeat on many pages (running heads/feet) are noise.
    def norm(t):
        return re.sub(r"\d+", "#", t.strip().lower())
    repeats = collections.Counter()
    for h, page_lines, _ in lines_by_page:
        for _, bbox, spans in page_lines:
            if isinstance(spans, tuple):
                continue
            if bbox[1] < h * 0.1 or bbox[3] > h * 0.9:
                repeats[norm("".join(s["text"] for s in spans))] += 1
    noise = {k for k, n in repeats.items() if n >= 3}

    toc_titles = {(p - 1, re.sub(r"\s+", " ", t.strip().lower())) for _, t, p in d.get_toc() if p > 0}
    out = []
    para = []
    last_block = None

    def end_para():
        if para:
            text = re.sub(r"\s+", " ", " ".join(para)).strip()
            text = re.sub(r"(\w)- (\w)", r"\1\2", text)  # rejoin hyphenated breaks
            if text:
                out.append(text)
            para.clear()

    for pno, (h, page_lines, pg) in enumerate(lines_by_page):
        for block, bbox, spans in page_lines:
            if isinstance(spans, tuple):
                kind, what = spans
                if kind == "table":
                    end_para()
                    width = max(len(r) for r in what)
                    for k, r in enumerate(what):
                        out.append("| " + " | ".join(r + [""] * (width - len(r))) + " |")
                        if k == 0:
                            out.append("|" + "---|" * width)
                    last_block = None
                    continue
                if kind == "image" and what:
                    mark = images.add_bytes(what)
                else:
                    mark = images.add_pixmap(pg.get_pixmap(clip=what, dpi=150, colorspace=pymupdf.csRGB)) if kind == "drawing" else ""
                if mark:
                    end_para()
                    out.append(mark)
                    last_block = None
                continue
            # A list bullet set in a symbol font with no text mapping comes
            # out as "?" (or a private-use code): make it a real bullet, and
            # its own paragraph, one per item.
            bullet = len(spans) > 1 and is_bullet(spans[0], spans[1])
            if bullet:
                spans = [dict(spans[0], text="• ")] + spans[1:]
            raw = "".join(s["text"] for s in spans)
            t = raw.strip()
            if bullet:
                end_para()
                last_block = block
            if not t or re.fullmatch(r"[\divxlcIVXLC\s.\-–—]{1,8}", t) and (bbox[1] < h * 0.1 or bbox[3] > h * 0.9):
                continue  # page number
            if (bbox[1] < h * 0.1 or bbox[3] > h * 0.9) and norm(raw) in noise:
                continue
            # The line's size is what most of its letters are set in (OCR text
            # gives single letters odd sizes).
            chars = sum(len(s["text"].strip()) for s in spans) or 1
            size = sum(s["size"] * len(s["text"].strip()) for s in spans) / chars
            letters = len(re.findall(r"[^\W\d_]", t))
            is_toc = (pno, re.sub(r"\s+", " ", t.lower())) in toc_titles
            if is_toc or (size >= body * 1.25 and letters >= 3 and len(t) < 90 and not t.endswith(("-", "‐"))):
                end_para()
                out.append("# " + clean(t).strip())
                last_block = None
                continue
            if block != last_block:
                end_para()
                last_block = block
            line = "".join(
                wrap(clean(s["text"]), bool(s["flags"] & 16) or "Bold" in s["font"], bool(s["flags"] & 2) or "Italic" in s["font"] or "Oblique" in s["font"])
                for s in spans
            )
            para.append(line.strip())
        end_para()
    # Words hyphenated across two text blocks meet only when join_broken
    # rejoins the blocks: close those breaks too.
    return title, "\n\n".join(re.sub(r"(\w)- (\w)", r"\1\2", p) for p in join_broken(out))


BULLET_GLYPHS = {"?", "\uf0b7", "\uf0a7", "\uf0d8", "\uf076", "\uf0fc", "\uf02d", "•", "·", "▪", "■", "◦", "●", "○", "♦", "❖", "➢", "\ufffd"}


def is_bullet(first, second):
    """True if `first` is a lone list marker before the line's text: one
    bullet-like glyph in a different font from the text that follows (a
    real "?" is in the text's own font)."""
    g = first["text"].strip()
    return g in BULLET_GLYPHS and (first["font"] != second["font"] or g != "?") and (second["text"].strip()[:1].isalnum() or second["text"].strip()[:1] in "(\"'“‘[")


def join_broken(paras):
    """Rejoins paragraphs a page break cut mid-sentence: the first doesn't
    end a sentence and the next starts in lower case (e.g. "codesto tuo" |
    "fatterello. Eh, …")."""
    out = []
    for p in paras:
        prev = out[-1] if out else ""
        start = p.lstrip("*_ ")
        if (prev and not prev.startswith(("# ", "{image:")) and not p.startswith(("# ", "{image:"))
                and not re.search(r"[.!?…:;»”\"')\]]\**$", prev) and start[:1].islower()):
            out[-1] = prev + " " + p
        else:
            out.append(p)
    return out


# ---------------------------------------------------------------- main

def slug(s: str) -> str:
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")[:48] or "book"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="+", type=Path)
    ap.add_argument("--out", type=Path, default=Path("books/private"))
    ap.add_argument("--sd", type=Path, help="write straight to the reader's SD card (its mount point, e.g. /Volumes/FASTREADER), every picture kept")
    ap.add_argument("--all-pictures", action="store_true", help="keep every picture (for the SD card) when writing to --out")
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    for f in a.files:
        ext = f.suffix.lower()
        images = Images(CARD_BUDGET if a.sd or a.all_pictures else IMG_BUDGET)
        if ext == ".epub":
            title, text = epub(f, images)
        elif ext == ".pdf":
            title, text = pdf(f, images)
        else:
            title, text = f.stem, clean(f.read_text(errors="replace"))
        # A short paragraph set entirely in bold (not italic) is a heading too,
        # e.g. "CHAPTER I" in a document that never changes font size.
        def promote(p):
            words = p.split()
            if words and len(p) < 60 and all(re.fullmatch(r"\*\*[^*]+\*\*", w) for w in words):
                return "# " + p.replace("**", "")
            return p
        # (Leading spaces kept: they are a nested list item's level.)
        text = "\n\n".join(promote(p.strip("\n").rstrip()) for p in text.split("\n\n"))
        # An opening bracket or quote before bold/italic markers goes inside
        # them ("(**check**)" -> "**(check**)"): the reader reads markers at
        # a word's start only.
        text = re.sub(r"([(\[“‘\"'«‹]+)(\*+)(?=\S)", r"\2\1", text)
        # Collapse 3+ blank lines; first line carries the title for the library.
        text = re.sub(r"\n{3,}", "\n\n", text).strip() + "\n"
        if a.sd:
            # The reader's card: BOOKS/BKnnnnnn.BK (8.3 names; the title is inside).
            books = a.sd / "BOOKS"
            books.mkdir(exist_ok=True)
            taken = [int(p.stem[2:]) for p in books.glob("BK*.BK") if p.stem[2:].isdigit()]
            dest = books / f"BK{max(taken, default=0) + 1:06d}.BK"
        else:
            dest = a.out / f"{slug(title)}.md"
        author = getattr(images, "author", "")
        head = f"<!-- title: {title} -->\n" + (f"<!-- author: {author} -->\n" if author else "")
        dest.write_bytes(f"{head}{text}".encode() + images.section())
        words = len(text.split())
        heads = text.count("\n# ") + text.startswith("# ")
        pics = f", {len(images.items)} pictures ({images.used // 1024} KB)" if images.items else ""
        print(f"{dest}  {words} words, {heads} headings{pics}  ({title})")


if __name__ == "__main__":
    sys.exit(main())

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
    SKIP = {"script", "style", "head", "sup", "img", "svg", "figure", "title"}

    def __init__(self, bold_classes, italic_classes):
        super().__init__(convert_charrefs=True)
        self.out = []
        self.line = []
        self.stack = []
        self.skip = 0
        self.heading = False
        self.bold_classes, self.italic_classes = bold_classes, italic_classes

    def style(self):
        b = any(t in ("b", "strong") or c & self.bold_classes for t, c in self.stack)
        i = any(t in ("i", "em", "cite") or c & self.italic_classes for t, c in self.stack)
        return b, i

    def flush(self):
        text = re.sub(r"\s+", " ", "".join(self.line)).strip()
        self.line = []
        if text:
            if self.heading:
                self.out.append("# " + re.sub(r"\*+", "", text))
            else:
                self.out.append(text)

    def handle_starttag(self, tag, attrs):
        classes = set((dict(attrs).get("class") or "").split())
        if tag in self.SKIP:
            self.skip += tag not in ("br", "img")
            return
        if tag in self.BLOCK or tag in self.HEAD:
            self.flush()
            if tag in self.HEAD:
                self.heading = True
        if tag != "br":
            self.stack.append((tag, classes))

    def handle_endtag(self, tag):
        if tag in self.SKIP:
            self.skip = max(0, self.skip - (tag not in ("br", "img")))
            return
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


def css_classes(css: str):
    bold, italic = set(), set()
    for sel, body in re.findall(r"([^{}]+)\{([^}]*)\}", css):
        names = set(re.findall(r"\.([\w-]+)", sel))
        if re.search(r"font-weight\s*:\s*(bold|[6-9]00)", body):
            bold |= names
        if re.search(r"font-style\s*:\s*(italic|oblique)", body):
            italic |= names
    return bold, italic


def epub(path: Path):
    z = zipfile.ZipFile(path)
    ns = {"c": "urn:oasis:names:tc:opendocument:xmlns:container", "o": "http://www.idpf.org/2007/opf", "dc": "http://purl.org/dc/elements/1.1/"}
    opf_path = ET.fromstring(z.read("META-INF/container.xml")).find(".//c:rootfile", ns).get("full-path")
    opf = ET.fromstring(z.read(opf_path))
    base = posixpath.dirname(opf_path)
    title_el = opf.find(".//dc:title", ns)
    title = title_el.text.strip() if title_el is not None and title_el.text else path.stem
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
        p = XhtmlText(bold, italic)
        p.feed(z.read(posixpath.join(base, html.unescape(href))).decode("utf-8", "replace"))
        p.flush()
        parts.extend(p.out)
    return title, "\n\n".join(parts)


# ---------------------------------------------------------------- PDF

def pdf(path: Path):
    import pymupdf

    d = pymupdf.open(path)
    title = (d.metadata or {}).get("title") or path.stem.split(" -- ")[0]
    # Body size = the size most characters are set in.
    sizes = collections.Counter()
    lines_by_page = []
    for pg in d:
        page_lines = []
        for b in pg.get_text("dict", flags=pymupdf.TEXT_PRESERVE_WHITESPACE)["blocks"]:
            for ln in b.get("lines", []):
                spans = [s for s in ln["spans"] if s["text"].strip()]
                if not spans:
                    continue
                for s in spans:
                    sizes[round(s["size"])] += len(s["text"])
                page_lines.append((b["number"], ln["bbox"], spans))
        lines_by_page.append((pg.rect.height, page_lines))
    body = sizes.most_common(1)[0][0] if sizes else 11

    # Lines that repeat on many pages (running heads/feet) are noise.
    def norm(t):
        return re.sub(r"\d+", "#", t.strip().lower())
    repeats = collections.Counter()
    for h, page_lines in lines_by_page:
        for _, bbox, spans in page_lines:
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

    for pno, (h, page_lines) in enumerate(lines_by_page):
        for block, bbox, spans in page_lines:
            raw = "".join(s["text"] for s in spans)
            t = raw.strip()
            if not t or re.fullmatch(r"[\divxlcIVXLC\s.\-–—]{1,8}", t) and (bbox[1] < h * 0.1 or bbox[3] > h * 0.9):
                continue  # page number
            if (bbox[1] < h * 0.1 or bbox[3] > h * 0.9) and norm(raw) in noise:
                continue
            size = max(round(s["size"]) for s in spans)
            is_toc = (pno, re.sub(r"\s+", " ", t.lower())) in toc_titles
            if is_toc or (size >= body * 1.25 and len(t) < 120):
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
        end_para()  # a page break ends the paragraph only if the block ends; good enough
    return title, "\n\n".join(out)


# ---------------------------------------------------------------- main

def slug(s: str) -> str:
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")[:48] or "book"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="+", type=Path)
    ap.add_argument("--out", type=Path, default=Path("books/private"))
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    for f in a.files:
        ext = f.suffix.lower()
        if ext == ".epub":
            title, text = epub(f)
        elif ext == ".pdf":
            title, text = pdf(f)
        else:
            title, text = f.stem, clean(f.read_text(errors="replace"))
        # A short paragraph set entirely in bold (not italic) is a heading too,
        # e.g. "CHAPTER I" in a document that never changes font size.
        def promote(p):
            words = p.split()
            if words and len(p) < 60 and all(re.fullmatch(r"\*\*[^*]+\*\*", w) for w in words):
                return "# " + p.replace("**", "")
            return p
        text = "\n\n".join(promote(p.strip()) for p in text.split("\n\n"))
        # Collapse 3+ blank lines; first line carries the title for the library.
        text = re.sub(r"\n{3,}", "\n\n", text).strip() + "\n"
        dest = a.out / f"{slug(title)}.md"
        dest.write_text(f"<!-- title: {title} -->\n{text}")
        words = len(text.split())
        heads = text.count("\n# ") + text.startswith("# ")
        print(f"{dest}  {words} words, {heads} headings  ({title})")


if __name__ == "__main__":
    sys.exit(main())

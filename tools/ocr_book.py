"""Turns a scanned PDF into a real EPUB: text read with LightOnOCR-2 (bbox),
every illustration cut from the page at full resolution, chapters, captions.

    tools/.venv/bin/python tools/ocr_book.py scan.pdf out.epub [--title T] [--author A] [--pages 1-20]

Uses Scan Studio's OCR add-on (its MLX Python and worker, unchanged): the
model reads each page image and answers Markdown, with every picture as
`![image](image_N.png)x0,y0,x1,y1` on a 0-999 grid. Each page's answer is
kept in a cache next to the output, so a stopped run resumes where it was.
"""

import argparse
import html
import json
import os
import re
import subprocess
import sys
import time
import uuid
import zipfile
from pathlib import Path

MLX_PY = Path.home() / "Library/Application Support/Scan Studio/envs/mlx-vlm/bin/python"
WORKER = Path.home() / "Scans/.scancut/studio/mlx_ocr_worker.py"
MODEL = "lightonai/LightOnOCR-2-1B-bbox"
READ_PX = 1540  # longest side of the page image the model reads (its training size)
FIG_PX = 1600  # longest side of a saved illustration
FIG = re.compile(r"!\[[^\]]*\]\([^)]*\)\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)")


class Reader:
    """Scan Studio's worker, kept running: one request per line."""

    def __init__(self):
        env = dict(os.environ, HF_HUB_OFFLINE="1", TOKENIZERS_PARALLELISM="false")
        self.p = subprocess.Popen([str(MLX_PY), "-I", str(WORKER)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, text=True, bufsize=1, env=env)

    def read(self, image: Path) -> str:
        self.p.stdin.write(json.dumps({"image": str(image), "model": MODEL, "prompt": " ", "max_tokens": 4096}) + "\n")
        self.p.stdin.flush()
        r = json.loads(self.p.stdout.readline())
        if "error" in r:
            raise RuntimeError(r["error"])
        return r["text"]


def ocr_pages(pdf: Path, cache: Path, pages):
    """Every page's Markdown (from the cache when already read)."""
    import pymupdf
    cache.mkdir(parents=True, exist_ok=True)
    doc = pymupdf.open(pdf)
    reader = None
    t0, done = time.time(), 0
    out = {}
    for n in pages:
        f = cache / f"p{n:04d}.md"
        if not f.exists():
            reader = reader or Reader()
            pg = doc[n - 1]
            z = READ_PX / max(pg.rect.width, pg.rect.height)
            img = cache / "page.png"
            pg.get_pixmap(matrix=pymupdf.Matrix(z, z), colorspace=pymupdf.csRGB).save(img)
            f.write_text(reader.read(img))
            done += 1
            per = (time.time() - t0) / done
            left = sum(1 for k in pages if not (cache / f"p{k:04d}.md").exists())
            if sys.stdout.isatty():
                # In a terminal: one loading bar, redrawn in place.
                total, frac = len(pages), (len(pages) - left) / len(pages)
                fill = int(frac * 40)
                print(f"\r[{'█' * fill}{'░' * (40 - fill)}] {frac * 100:5.1f}%  page {total - left}/{total}  {per:.0f} s/page  ~{left * per / 60:.0f} min left ", end="", flush=True)
            else:
                print(f"page {n}: {per:.1f} s/page, {left} to go, about {left * per / 60:.0f} min left", flush=True)
        out[n] = f.read_text()
    return out


def page_blocks(n, md, doc, figdir: Path):
    """A page's Markdown → blocks: ('h', text), ('p', text), ('fig', file, caption), ('table', html)."""
    import pymupdf
    blocks = []
    paras = [p.strip() for p in re.split(r"\n\s*\n", md) if p.strip()]
    # Running head / page number at the top or bottom ("28 Capitolo primo", "Capitolo primo 29", "29").
    head = re.compile(r"^(\d{1,4}\s+\S.{0,60}|.{0,60}\S\s+\d{1,4}|\d{1,4}|[ivxlcdm]{1,6})$", re.I)
    if paras and head.match(paras[0]) and not paras[0].startswith("#") and not FIG.search(paras[0]):
        paras = paras[1:]
    if paras and re.fullmatch(r"\d{1,4}|[ivxlcdm]{1,6}", paras[-1], re.I):
        paras = paras[:-1]
    figs = 0
    for p in paras:
        m = FIG.search(p)
        if m:
            x0, y0, x1, y1 = (int(v) / 999 for v in m.groups())
            pg = doc[n - 1]
            r = pymupdf.Rect(x0 * pg.rect.width, y0 * pg.rect.height, x1 * pg.rect.width, y1 * pg.rect.height) & pg.rect
            if r.width > 20 and r.height > 20:
                figs += 1
                z = min(4, FIG_PX / max(r.width, r.height))
                name = f"p{n:04d}-{figs}.jpg"
                pg.get_pixmap(matrix=pymupdf.Matrix(z, z), clip=r, colorspace=pymupdf.csRGB).save(figdir / name, jpg_quality=88)
                blocks.append(("fig", name, ""))
            rest = FIG.sub("", p).strip()
            if rest:
                blocks.append(("p", rest))
        elif p.startswith("#"):
            blocks.append(("h", p.lstrip("#").strip()))
        elif p.lower().startswith("<table"):
            blocks.append(("table", p))
        else:
            blocks.append(("p", re.sub(r"\s*\n\s*", " ", p)))
    # A caption ("20 Altamira (Spagna), Bisonte…") sits right before or after its picture.
    cap = re.compile(r"^\d{1,4}[a-z]?\s+\S")
    for i, b in enumerate(blocks):
        if b[0] != "fig":
            continue
        for j in (i - 1, i + 1):
            if 0 <= j < len(blocks) and blocks[j][0] == "p" and cap.match(blocks[j][1]) and len(blocks[j][1]) < 300:
                blocks[i] = ("fig", b[1], blocks[j][1])
                blocks[j] = ("drop",)
                break
    return [b for b in blocks if b[0] != "drop"]


def join_pages(blocks):
    """A paragraph cut by a page break continues: join when the first doesn't
    end a sentence and the next starts in lower case (or after a hyphen)."""
    out = []
    for b in blocks:
        if b[0] == "p" and out:
            # the last text paragraph before any pictures in between
            k = len(out) - 1
            while k >= 0 and out[k][0] == "fig":
                k -= 1
            prev = out[k] if k >= 0 else None
            if prev and prev[0] == "p":
                a, t = prev[1], b[1]
                if a.endswith("-") and t[:1].islower():
                    out[k] = ("p", a[:-1] + t)
                    continue
                if not re.search(r"[.!?…:;»”\"')\]]\**$", a) and t.lstrip("*_«\"")[:1].islower():
                    out[k] = ("p", a + " " + t)
                    continue
        out.append(b)
    return out


def inline(t):
    """Markdown emphasis → XHTML, text escaped."""
    t = html.escape(t, quote=False)
    t = re.sub(r"\*\*\*(.+?)\*\*\*", r"<b><i>\1</i></b>", t)
    t = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", t)
    t = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<i>\1</i>", t)
    return t


def table_xhtml(t):
    """An HTML table from the model, made well-formed XHTML (or plain text)."""
    import xml.etree.ElementTree as ET
    t = re.sub(r"<br\s*>", "<br/>", t)
    try:
        ET.fromstring(t)
        return t
    except ET.ParseError:
        return "<p>" + html.escape(re.sub(r"<[^>]+>", " ", t)) + "</p>"


def build_epub(blocks, figdir: Path, out: Path, title, author, lang="it"):
    # Chapters start at headings; a short run of headings (title pages) stays together.
    chapters, cur = [], {"title": title, "body": []}
    for b in blocks:
        if b[0] == "h" and cur["body"] and any(x[0] in ("p", "fig") for x in cur["body"]):
            chapters.append(cur)
            cur = {"title": b[1], "body": []}
        elif b[0] == "h" and not any(x[0] in ("p", "fig") for x in cur["body"]):
            # Headings in a row open one chapter: "Capitolo primo: Strani inizi".
            heads = [x[1] for x in cur["body"] if x[0] == "h"]
            cur["title"] = ": ".join(heads + [b[1]]) if heads and len(heads) < 2 else (cur["title"] if heads else b[1])
        cur["body"].append(b)
    chapters.append(cur)

    def page(ch):
        parts = []
        for b in ch["body"]:
            if b[0] == "h":
                parts.append(f"<h2>{inline(b[1])}</h2>")
            elif b[0] == "p":
                parts.append(f"<p>{inline(b[1])}</p>")
            elif b[0] == "fig":
                cap = f"<figcaption>{inline(b[2])}</figcaption>" if b[2] else ""
                alt = html.escape(b[2] or "illustrazione", quote=True)
                parts.append(f'<figure><img src="images/{b[1]}" alt="{alt}"/>{cap}</figure>')
            elif b[0] == "table":
                parts.append(table_xhtml(b[1]))
        body = "\n".join(parts)
        return (f'<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="{lang}" lang="{lang}">'
                f'<head><meta charset="utf-8"/><title>{html.escape(ch["title"])}</title><link rel="stylesheet" href="style.css"/></head>'
                f'<body>\n{body}\n</body></html>')

    css = ("body{font-family:serif;line-height:1.45;margin:0 4%} h2{text-align:center;margin:1.5em 0 1em}"
           " p{text-indent:1.2em;margin:0 0 .3em;text-align:justify} figure{margin:1.2em 0;text-align:center;page-break-inside:avoid}"
           " img{max-width:100%;max-height:90vh} figcaption{font-size:.85em;font-style:italic;margin-top:.4em}"
           " table{border-collapse:collapse;font-size:.75em;margin:1em auto} td,th{border:1px solid #999;padding:.2em .4em}")
    bid = str(uuid.uuid5(uuid.NAMESPACE_URL, f"fastreader:{title}:{author}"))
    images = sorted({b[1] for b in blocks if b[0] == "fig"})
    with zipfile.ZipFile(out, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
                   '<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>', zipfile.ZIP_DEFLATED)
        z.writestr("OEBPS/style.css", css, zipfile.ZIP_DEFLATED)
        for i, ch in enumerate(chapters):
            z.writestr(f"OEBPS/c{i:03d}.xhtml", page(ch), zipfile.ZIP_DEFLATED)
        for name in images:
            z.write(figdir / name, f"OEBPS/images/{name}", zipfile.ZIP_STORED)
        nav = "".join(f'<li><a href="c{i:03d}.xhtml">{html.escape(ch["title"])}</a></li>' for i, ch in enumerate(chapters))
        z.writestr("OEBPS/nav.xhtml", f'<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html>\n<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="{lang}">'
                   f'<head><meta charset="utf-8"/><title>{html.escape(title)}</title></head><body><nav epub:type="toc" id="toc"><h1>Indice</h1><ol>{nav}</ol></nav></body></html>', zipfile.ZIP_DEFLATED)
        manifest = ['<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
                    '<item id="css" href="style.css" media-type="text/css"/>']
        manifest += [f'<item id="c{i:03d}" href="c{i:03d}.xhtml" media-type="application/xhtml+xml"/>' for i in range(len(chapters))]
        manifest += [f'<item id="i{k}" href="images/{n}" media-type="image/jpeg"/>' for k, n in enumerate(images)]
        spine = "".join(f'<itemref idref="c{i:03d}"/>' for i in range(len(chapters)))
        modified = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        z.writestr("OEBPS/content.opf", f'<?xml version="1.0" encoding="utf-8"?>\n<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bid">'
                   f'<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bid">urn:uuid:{bid}</dc:identifier>'
                   f'<dc:title>{html.escape(title)}</dc:title><dc:creator>{html.escape(author)}</dc:creator><dc:language>{lang}</dc:language>'
                   f'<meta property="dcterms:modified">{modified}</meta></metadata>'
                   f'<manifest>{"".join(manifest)}</manifest><spine>{spine}</spine></package>', zipfile.ZIP_DEFLATED)
    return len(chapters), len(images)


def main():
    import pymupdf
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pdf", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--title")
    ap.add_argument("--author", default="")
    ap.add_argument("--pages", help="e.g. 1-20 (default: all)")
    a = ap.parse_args()
    doc = pymupdf.open(a.pdf)
    lo, hi = (map(int, a.pages.split("-")) if a.pages else (1, doc.page_count))
    pages = list(range(lo, min(hi, doc.page_count) + 1))
    cache = a.out.with_suffix(".ocr-cache")
    md = ocr_pages(a.pdf, cache, pages)
    if sys.stdout.isatty():
        print()
    figdir = cache / "figures"
    figdir.mkdir(exist_ok=True)
    blocks = []
    for n in pages:
        blocks += page_blocks(n, md[n], doc, figdir)
    blocks = join_pages(blocks)
    chapters, images = build_epub(blocks, figdir, a.out, a.title or a.pdf.stem, a.author)
    words = sum(len(b[1].split()) for b in blocks if b[0] == "p")
    print(f"{a.out}: {words} words, {chapters} chapters, {images} illustrations")


if __name__ == "__main__":
    sys.exit(main())

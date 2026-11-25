// ---- converters begin (tools/build_demo.sh copies this part to the browser demo)
// ---- add a book: EPUBs are converted here in the browser (unzip + XHTML →
// the reader's markup), so a phone on the reader's Wi-Fi needs nothing else.
async function unzip(buf) {
  const v = new DataView(buf), files = {};
  let e = buf.byteLength - 22;
  while (e > 0 && v.getUint32(e, true) !== 0x06054b50) e--;
  let p = v.getUint32(e + 16, true);
  const n = v.getUint16(e + 10, true);
  for (let i = 0; i < n; i++) {
    const method = v.getUint16(p + 10, true), size = v.getUint32(p + 20, true);
    const nl = v.getUint16(p + 28, true), xl = v.getUint16(p + 30, true), cl = v.getUint16(p + 32, true);
    const local = v.getUint32(p + 42, true);
    const name = new TextDecoder().decode(new Uint8Array(buf, p + 46, nl));
    const start = local + 30 + v.getUint16(local + 26, true) + v.getUint16(local + 28, true);
    const data = new Uint8Array(buf, start, size);
    files[name] = method === 0 ? data
      : new Uint8Array(await new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('deflate-raw'))).arrayBuffer());
    p += 46 + nl + xl + cl;
  }
  return files;
}
function toMarkup(doc, docPath) {
  const out = [];
  // A publisher's pages, not the book (imprint, colophon, copyright, list of
  // illustrations, title pages; marked so in EPUB 3): skipped.
  const typed = [...doc.querySelectorAll('body,section,nav')].map(e => e.getAttribute('epub:type') || '').join(' ');
  if (/\b(imprint|colophon|copyright-page|loi|halftitlepage|titlepage)\b/.test(typed)) return '';
  // Verse (a poem or song in a <blockquote>): kept line by line.
  const isVerse = el => el.localName === 'blockquote' && /poem|song|verse|hymn|lyric/.test((el.getAttribute('class') || '') + ' ' + (el.getAttribute('epub:type') || ''));
  const dir = docPath.includes('/') ? docPath.replace(/[^/]*$/, '') : '';
  const resolve = src => {
    const parts = [];
    for (const seg of (dir + decodeURIComponent(src.split('#')[0])).split('/')) {
      if (seg === '..') parts.pop(); else if (seg && seg !== '.') parts.push(seg);
    }
    return parts.join('/');
  };
  const walk = (node, b, i) => {
    let t = '';
    for (const c of node.childNodes) {
      if (c.nodeType === 3) {
        const words = c.textContent.replace(/[*_]/g, '').split(/\s+/).filter(Boolean);
        const m = b && i ? '***' : b ? '**' : i ? '*' : '';
        t += (c.textContent.match(/^\s/) ? ' ' : '') + words.map(w => m + w + m).join(' ') + (c.textContent.match(/\s$/) ? ' ' : '');
      } else if (c.nodeType === 1) {
        const tag = c.localName;
        if (tag === 'br') { t += '\n'; continue; }
        if (['script', 'style', 'sup', 'img', 'svg'].includes(tag)) continue;
        t += walk(c, b || tag === 'b' || tag === 'strong', i || tag === 'i' || tag === 'em' || tag === 'cite');
      }
    }
    return t;
  };
  for (const el of doc.querySelectorAll('h1,h2,h3,h4,p,li,blockquote,img,image,figcaption,table')) {
    // A table: one "| a | b |" line per row, the header marked by "|---|".
    if (el.localName === 'table') {
      if (el.parentElement && el.parentElement.closest('table')) continue;
      const rows = [...el.querySelectorAll('tr')].filter(tr => tr.closest('table') === el)
        .map(tr => [...tr.children].filter(c => /^t[dh]$/.test(c.localName)).map(c => walk(c, false, false).replace(/\s+/g, ' ').replace(/\|/g, '/').trim()))
        .filter(r => r.some(Boolean));
      if (!rows.length) continue;
      const width = Math.max(...rows.map(r => r.length));
      const head = el.querySelector('th') && [...el.querySelectorAll('tr')][0]?.querySelector('th');
      rows.forEach((r, k) => {
        out.push('| ' + [...r, ...Array(width - r.length).fill('')].join(' | ') + ' |');
        if (k === 0 && head) out.push('|' + '---|'.repeat(width));
      });
      continue;
    }
    if (el.closest('table')) continue;
    if (el.localName === 'figcaption') {
      // A picture's description: kept with the picture, not read as text.
      const t = walk(el, false, false).replace(/\s+/g, ' ').trim();
      if (t) out.push(/^\u0000IMG:/.test(out[out.length - 1] || '') ? '{caption} ' + t + ' {/caption}' : t);
      continue;
    }
    if (el.closest('figcaption')) continue;
    if (el.localName === 'img' || el.localName === 'image') {
      const src = el.getAttribute('src') || el.getAttribute('xlink:href') || el.getAttribute('href');
      if (src) out.push('\u0000IMG:' + resolve(src) + '\u0000');
      continue;
    }
    if (el.parentElement && el.parentElement.closest('blockquote') && isVerse(el.parentElement.closest('blockquote'))) continue;
    if (isVerse(el)) {
      const lines = walk(el, false, false).split('\n').map(l => l.replace(/\s+/g, ' ').trim()).filter(Boolean);
      if (lines.length) out.push('{verse}\n' + lines.join('\n') + '\n{/verse}');
      continue;
    }
    if (el.closest('p,li,blockquote') && el.closest('p,li,blockquote') !== el) continue;
    const text = walk(el, false, false).replace(/\s+/g, ' ').trim();
    if (!text) continue;
    // A list item: its marker first ("•", or its number in an <ol>).
    let marker = '';
    if (el.localName === 'li') {
      const list = el.parentElement;
      // Nested lists: two spaces of indentation per level.
      let depth = 0;
      for (let p = list && list.parentElement; p; p = p.parentElement) if (p.localName === 'ul' || p.localName === 'ol') depth++;
      const pad = '  '.repeat(depth);
      if (list && list.localName === 'ol') {
        const items = [...list.children].filter(c => c.localName === 'li');
        marker = pad + (parseInt(list.getAttribute('start') || '1', 10) + items.indexOf(el)) + '. ';
      } else marker = pad + '• ';
    }
    out.push(/^h[1-4]$/.test(el.localName) ? '# ' + text.replace(/\*/g, '') : marker + text);
  }
  // An opening bracket or quote before bold/italic markers goes inside them
  // ("(**check**)" -> "**(check**)"): the reader reads markers at a word's start.
  // Markers starting mid-word ("rattle-*snake*,") move to its start too.
  return out.join('\n\n').replace(/([(\[“‘"'«‹]+)(\*+)(?=\S)/g, '$2$1')
    .replace(/(?<![\S*])([^\s*]+)(\*+)(?=[^\s*]\S*?\*)/g, '$2$1');
}
// Pictures go after the text: "\n\0FRIMG1", count, then width, height and
// RGB565 pixels per picture, scaled to fit the reader's stage (300×190).
class Pictures {
  // `budget`: bytes of pixels the book may carry (small without an SD card).
  constructor(budget = 2e6) { this.list = []; this.used = 0; this.budget = budget; this.maxArea = 300 * 190; this.wanted = 0; this.seen = 0; this.n = 0; this.room = Infinity; }
  // Shares the budget among `n` pictures: smaller pictures rather than only
  // the first ones (an illustrated book has hundreds). If even at the
  // smallest useful size they don't all fit, keep ones spread evenly
  // through the book.
  plan(n) {
    this.n = n;
    this.maxArea = Math.max(140 * 90, Math.min(300 * 190, this.budget / 2 / Math.max(1, n)));
    this.room = Math.floor(this.budget / 2 / this.maxArea);
  }
  // Called once per candidate, in book order: is this one among those kept?
  pick() {
    const i = this.seen++;
    return this.n <= this.room || Math.floor(i * this.room / this.n) !== Math.floor((i + 1) * this.room / this.n);
  }
  full() { return this.used > this.budget; }
  // Adds the region (sx, sy, sw, sh) of any drawable; returns "{image:N}" or "".
  add(src, sx, sy, sw, sh) {
    if (sw < 48 || sh < 48) return ''; // rules, bullets, icons
    let k = Math.min(300 / sw, 190 / sh, 1);
    if (sw * k * sh * k > this.maxArea) k *= Math.sqrt(this.maxArea / (sw * k * sh * k));
    const w = Math.max(1, Math.round(sw * k)), h = Math.max(1, Math.round(sh * k));
    if (this.used + w * h * 2 > this.budget) return '';
    const cv = new OffscreenCanvas(w, h), g = cv.getContext('2d');
    g.fillStyle = '#fff'; g.fillRect(0, 0, w, h);
    g.drawImage(src, sx, sy, sw, sh, 0, 0, w, h);
    const rgba = g.getImageData(0, 0, w, h).data, px = new Uint8Array(4 + w * h * 2);
    // Nearly all white: an empty stretch of page, not a picture.
    let ink = 0;
    for (let i = 0; i < rgba.length; i += 16) if (rgba[i] + rgba[i + 1] + rgba[i + 2] < 600) ink++;
    if (ink < rgba.length / 16 * 0.03) return '';
    this.wanted++;
    const dv = new DataView(px.buffer);
    dv.setUint16(0, w, true); dv.setUint16(2, h, true);
    for (let i = 0, o = 4; i < rgba.length; i += 4, o += 2) dv.setUint16(o, (rgba[i] >> 3) << 11 | (rgba[i + 1] >> 2) << 5 | rgba[i + 2] >> 3, true);
    this.list.push(px);
    this.used += w * h * 2;
    return `{image:${this.list.length - 1}}`;
  }
  // Version 2: a table of (width, height, offset) first, so the reader can
  // fetch any one picture from its card without reading the others.
  blob() {
    if (!this.list.length) return null;
    const n = this.list.length, head = new Uint8Array(4 + 8 * n), dv = new DataView(head.buffer);
    dv.setUint16(0, n, true);
    let off = head.length;
    this.list.forEach((px, k) => {
      const v = new DataView(px.buffer);
      dv.setUint16(4 + 8 * k, v.getUint16(0, true), true);
      dv.setUint16(6 + 8 * k, v.getUint16(2, true), true);
      dv.setUint32(8 + 8 * k, off, true);
      off += px.length - 4;
    });
    return new Blob([new Uint8Array([10, 0, 70, 82, 73, 77, 71, 50]), head, ...this.list.map(px => px.subarray(4))]);
  }
}

// With an SD card in the reader, a book keeps every picture at full size.
function pictureBudget() {
  return S && S.card ? Math.max(2e6, (S.upload_limit || 0) - 4e6) : 2e6;
}

// Paragraphs a page break cut mid-sentence: the first doesn't end a sentence
// and the next starts in lower case. Same rule as tools/convert.py.
const BULLETS = new Set(['?', '\uf0b7', '\uf0a7', '\uf0d8', '\uf076', '\uf0fc', '\uf02d', '•', '·', '▪', '■', '◦', '●', '○', '♦', '❖', '➢', '\ufffd']);
// A lone list marker before the line's text, in a different font from it (a
// real "?" is in the text's own font).
function isBullet(first, second) {
  const g = first.text.trim(), next = second.text.trim().charAt(0);
  return BULLETS.has(g) && (first.font !== second.font || g !== '?') && /[\p{L}\p{N}("'“‘\[]/u.test(next);
}
function joinBroken(paras) {
  const out = [];
  for (const p of paras) {
    const prev = out[out.length - 1];
    const special = t => t.startsWith('# ') || t.startsWith('{image:');
    if (prev && !special(prev) && !special(p) && !/[.!?…:;»”"')\]]\**$/.test(prev) && /^[*_ ]*\p{Ll}/u.test(p))
      out[out.length - 1] = prev + ' ' + p;
    else out.push(p);
  }
  return out;
}

// ---- PDFs, with pdf.js served by the reader itself (no internet needed).
async function pdfToMarkup(file, progress) {
  const base = globalThis.PDFJS_BASE || '/pdfjs/';
  const pdfjs = await import(base + 'pdf.min.mjs');
  pdfjs.GlobalWorkerOptions.workerSrc = base + 'pdf.worker.min.mjs';
  // Keep the loading task: closing a document goes through it.
  let task = null;
  const open = async () => {
    task = pdfjs.getDocument({data: new Uint8Array(await file.arrayBuffer()), fontExtraProperties: true});
    return task.promise;
  };
  let doc = await open();
  const meta = await doc.getMetadata().catch(() => null);
  const title = (meta?.info?.Title || file.name.replace(/\.pdf$/i, '').split(' -- ')[0]).trim();
  const author = (meta?.info?.Author || '').trim();
  const norm = t => t.trim().toLowerCase().replace(/\s+/g, ' ');
  const tocTitles = new Set();
  const walkToc = items => (items || []).forEach(o => { tocTitles.add(norm(o.title)); walkToc(o.items); });
  walkToc(await doc.getOutline().catch(() => null));

  // 1. Every page's lines: text spans with size, style, position.
  const pages = [], sizes = new Map();
  // Scanned books: after sampling the first pages, stop reading drawing
  // commands (that decodes every page photo: ~600 MB for 700 pages, more
  // than a phone allows a tab).
  let sampled = 0, scannedPages = 0, scannedBook = false;
  for (let n = 1; n <= doc.numPages; n++) {
    progress(`Reading page ${n} of ${doc.numPages}…`);
    const page = await doc.getPage(n);
    const vp = page.getViewport({scale: 1});
    const ops = scannedBook ? {fnArray: [], argsArray: []} : await page.getOperatorList();
    // PDF space → page pixels (top-left origin), whatever the page's offset or rotation.
    const toView = (x, y) => { const m = vp.transform; return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]; };
    const fontStyle = name => {
      // pdf.js reads bold/italic from the font itself (descriptor flags, weight, name).
      try {
        if (page.commonObjs.has(name)) {
          // The real name ("AAAAAD+Arial-ItalicMT") needs fontExtraProperties.
          const f = page.commonObjs.get(name), real = f.name || '';
          return {bold: !!(f.bold || f.black) || /bold|black|heavy|semibold|demi/i.test(real), italic: !!f.italic || /italic|oblique/i.test(real)};
        }
      } catch (e) {}
      return {bold: false, italic: false};
    };
    const tc = await page.getTextContent();
    const lines = [];
    let line = null;
    for (const it of tc.items) {
      if (!it.str && !it.hasEOL) continue;
      const t = it.transform, size = Math.hypot(t[2], t[3]) || Math.hypot(t[0], t[1]);
      const [x, y] = toView(t[4], t[5]);
      if (!line || Math.abs(y - line.y) > size * 0.5) {
        line = {y, x, size, spans: [], indent: 0, started: false, chars: 0, weighted: 0};
        lines.push(line);
      }
      // Blank space before the first letters (some PDFs indent with spaces).
      if (it.str && !line.started) {
        const lead = it.str.length - it.str.trimStart().length;
        line.indent += it.str.length ? it.width * lead / it.str.length : 0;
        line.started = lead < it.str.length;
      }
      if (it.str) {
        line.spans.push({text: it.str, font: it.fontName, ...fontStyle(it.fontName)});
        // The line's size is what most of its letters are set in: OCR text
        // gives single letters odd sizes, and the largest would mislead.
        const k = it.str.trim().length;
        line.chars += k; line.weighted += size * k;
        line.size = line.chars ? line.weighted / line.chars : size;
        if (it.str.trim()) sizes.set(Math.round(size), (sizes.get(Math.round(size)) || 0) + it.str.length);
      }
      if (it.hasEOL) line = null;
    }
    // Pictures: follow the drawing state to find where each image lands.
    const boxes = [];
    let ctm = [1, 0, 0, 1, 0, 0];
    const stack = [];
    const mul = (m, n) => [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3], m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]];
    const O = pdfjs.OPS;
    ops.fnArray.forEach((fn, i) => {
      if (fn === O.save) stack.push(ctm);
      else if (fn === O.restore) ctm = stack.pop() || ctm;
      else if (fn === O.transform) ctm = mul(ctm, ops.argsArray[i]);
      else if (fn === O.paintImageXObject || fn === O.paintInlineImageXObject || fn === O.paintImageXObjectRepeat) {
        const xs = [ctm[4], ctm[4] + ctm[0], ctm[4] + ctm[2], ctm[4] + ctm[0] + ctm[2]];
        const ys = [ctm[5], ctm[5] + ctm[1], ctm[5] + ctm[3], ctm[5] + ctm[1] + ctm[3]];
        const pts = xs.map((x, k) => toView(x, ys[k]));
        const [x0, x1] = [Math.min(...pts.map(q => q[0])), Math.max(...pts.map(q => q[0]))];
        const [y0, y1] = [Math.min(...pts.map(q => q[1])), Math.max(...pts.map(q => q[1]))];
        if (x1 - x0 > vp.width * 0.15 && y1 - y0 > vp.height * 0.08) boxes.push([x0, y0, x1 - x0, y1 - y0]);
      }
    });
    // A scanned book: each page is one big photo with the OCR text on top.
    // That photo is the page itself, not an illustration: keep the text only.
    const hasText = lines.some(l => l.spans.some(sp => sp.text.trim()));
    const scan = b => b[2] * b[3] > vp.width * vp.height * 0.6;
    if (!scannedBook) {
      sampled++;
      if (boxes.some(scan)) scannedPages++;
      if (sampled === 8 && scannedPages >= 6) {
        scannedBook = true;
        // A long scanned book needs over a gigabyte to cut its pictures out:
        // more than a phone gives a browser tab (it reloads the page).
        const phone = navigator.userAgentData?.mobile ?? /iPhone|iPad|Android|Mobile/i.test(navigator.userAgent);
        if (phone && doc.numPages > 150 && !confirm(`"${title}" is a scanned book of ${doc.numPages} pages: too heavy to convert on a phone, which would run out of memory part-way.\n\nBetter: join the FastReader Wi-Fi with a computer and open http://192.168.4.1 there, or convert it on the computer with tools/convert.py and copy it to the SD card.\n\nTry on this phone anyway?`))
          throw new Error('too heavy for a phone: convert it on a computer (same page, http://192.168.4.1)');
      }
    }
    // Without drawing commands (rest of a scanned book): a page without
    // text is a plate or the cover, kept whole.
    if (scannedBook && !ops.fnArray.length && !hasText) boxes.push([0, 0, vp.width, vp.height]);
    const scanned = hasText && (boxes.some(scan) || (scannedBook && !ops.fnArray.length));
    const kept = hasText ? boxes.filter(b => !scan(b)) : boxes;
    if (scanned) {
      // Illustrations inside a scanned page are part of its photo: a tall
      // stretch without text lines is one. Cut it out of the page.
      const ys = lines.filter(l => l.spans.some(sp => sp.text.trim())).map(l => [l.y - l.size, l.y + l.size * 0.3]).sort((a, b) => a[0] - b[0]);
      let top = vp.height * 0.06;
      for (const [a, b] of [...ys, [vp.height * 0.94, vp.height]]) {
        if (a - top > vp.height * 0.2) kept.push([vp.width * 0.06, top, vp.width * 0.88, a - top]);
        top = Math.max(top, b);
      }
    }
    pages.push({n, h: vp.height, lines, boxes: kept});
    // Memory: reading a page's drawing commands decodes its images (on a
    // scanned book, a photo of every page). Let them go now, or a phone
    // runs out of memory and reloads the page part-way.
    page.cleanup();
    if (n % 40 === 0) await doc.cleanup();
  }
  if (!sizes.size) throw new Error('this PDF is scanned pages without text: run OCR on it first (e.g. in Acrobat or ocrmypdf)');
  let body = 11, best = 0;
  sizes.forEach((count, size) => { if (count > best) { best = count; body = size; } });

  // 2. Running heads and feet repeat across pages: drop them.
  const key = l => l.spans.map(s => s.text).join('').trim().toLowerCase().replace(/\d+/g, '#');
  const edge = (p, l) => l.y < p.h * 0.1 || l.y > p.h * 0.9;
  const repeats = new Map();
  for (const p of pages) for (const l of p.lines) if (edge(p, l)) repeats.set(key(l), (repeats.get(key(l)) || 0) + 1);

  // 3. Lines → paragraphs, headings and pictures.
  let rendered = 0;
  const pics = new Pictures(pictureBudget());
  pics.plan(pages.reduce((n, p) => n + p.boxes.length, 0));
  const out = [];
  let para = [];
  const endPara = () => {
    const t = para.join(' ').replace(/\s+/g, ' ').replace(/(\p{L})- (\p{Ll})/gu, '$1$2').trim();
    if (t) out.push(t);
    para = [];
  };
  const styled = s => s.text.split(/(\s+)/).map(w => {
    if (!w.trim()) return w;
    const m = s.bold && s.italic ? '***' : s.bold ? '**' : s.italic ? '*' : '';
    return m + w.replace(/[*_]/g, '') + m;
  }).join('');
  for (let n = 0; n < pages.length; n++) {
    const p = pages[n];
    progress(`Laying out page ${n + 1} of ${pages.length}…`);
    const lines = p.lines.filter(l => {
      const t = l.spans.map(s => s.text).join('').trim();
      if (!t || !/[\p{L}\p{N}]/u.test(t)) return false; // empty or pure symbols (OCR noise in pictures)
      if (edge(p, l) && (/^[\divxlcIVXLC\s.\-–—]{1,8}$/.test(t) || (repeats.get(key(l)) || 0) >= 3)) return false;
      return true;
    });
    const lefts = lines.map(l => Math.round(l.x));
    const left = lefts.length ? lefts.sort((a, b) => a - b)[Math.floor(lefts.length * 0.2)] : 0;
    const gaps = lines.slice(1).map((l, i) => l.y - lines[i].y).filter(g => g > 0).sort((a, b) => a - b);
    const lineGap = gaps.length ? gaps[Math.floor(gaps.length / 2)] : body * 1.2;
    // Pictures go before the first line below their top.
    let render = null, page = null;
    const boxes = p.boxes.slice().sort((a, b) => a[1] - b[1]);
    const picture = async box => {
      if (!pics.pick() || pics.full()) return '';
      if (!render) {
        // Only pages with pictures are opened again, rendered just big
        // enough for a sharp 300-pixel picture.
        page = await doc.getPage(p.n);
        const widest = Math.max(...p.boxes.map(b => b[2]));
        const scale = Math.min(2, Math.max(1, 600 / widest)), vp = page.getViewport({scale});
        const cv = new OffscreenCanvas(Math.ceil(vp.width), Math.ceil(vp.height));
        await page.render({canvasContext: cv.getContext('2d'), viewport: vp, canvas: cv}).promise;
        render = {cv, scale};
      }
      const k = render.scale;
      return pics.add(render.cv, box[0] * k, box[1] * k, box[2] * k, box[3] * k);
    };
    let prev = null;
    for (const l of lines) {
      while (boxes.length && boxes[0][1] <= l.y) {
        const mark = await picture(boxes.shift());
        if (mark) { endPara(); out.push(mark); }
      }
      // A list bullet in a symbol font with no text mapping arrives as "?"
      // (or a private-use code): a real bullet, starting its own paragraph.
      const sp = l.spans.filter(s => s.text.trim());
      const bullet = sp.length > 1 && isBullet(sp[0], sp[1]);
      if (bullet) { sp[0].text = '• '; l.spans = sp; endPara(); prev = null; }
      const t = l.spans.map(s => s.text).join('').replace(/\s+/g, ' ').trim();
      const letters = (t.match(/\p{L}/gu) || []).length;
      if (tocTitles.has(norm(t)) || (l.size >= body * 1.25 && letters >= 3 && t.length < 90 && !/[-‐]$/.test(t))) {
        endPara();
        out.push('# ' + t.replace(/[*_]/g, ''));
        prev = null;
        continue;
      }
      const indented = l.x + l.indent > left + body * 0.6;
      const gap = prev ? l.y - prev.y : 0;
      if (!bullet && (!prev || indented || gap > lineGap * 1.6)) endPara();
      para.push(l.spans.map(styled).join('').trim());
      prev = l;
    }
    for (const box of boxes) {
      const mark = await picture(box);
      if (mark) { endPara(); out.push(mark); }
    }
    endPara();
    if (render) { render.cv.width = render.cv.height = 0; render = null; }
    if (page) {
      page.cleanup();
      // pdf.js keeps memory from every page it renders (a scanned page's
      // photo is ~40 MB decoded) and its own cleanup doesn't give it back:
      // reopen the PDF every 40 rendered pages, so a phone never runs out.
      if (++rendered % 40 === 0) {
        await task.destroy();
        doc = await open();
      }
    }
  }
  // A short paragraph all in bold is a heading too ("CHAPTER I").
  const paras = out.map(t => (t.length < 60 && /^(\*\*[^*]+\*\*\s*)+$/.test(t)) ? '# ' + t.replace(/\*/g, '') : t);
  // Words hyphenated across two text blocks meet only in joinBroken.
  return {title, author, text: joinBroken(paras).map(p => p.replace(/(\p{L})- (\p{Ll})/gu, '$1$2')).join('\n\n'), images: pics.blob(), pictures: pics.list.length, wanted: pics.wanted};
}

async function epubToMarkup(file) {
  const files = await unzip(await file.arrayBuffer());
  const xml = s => new DOMParser().parseFromString(new TextDecoder().decode(s), 'application/xhtml+xml');
  const opfPath = xml(files['META-INF/container.xml']).querySelector('rootfile').getAttribute('full-path');
  const opf = xml(files[opfPath]), base = opfPath.includes('/') ? opfPath.replace(/[^/]*$/, '') : '';
  const title = (opf.querySelector('title')?.textContent || file.name).trim();
  const author = (opf.querySelector('creator')?.textContent || '').trim();
  const items = {};
  opf.querySelectorAll('manifest item').forEach(it => items[it.getAttribute('id')] = it.getAttribute('href'));
  const parts = [];
  for (const ref of opf.querySelectorAll('spine itemref')) {
    const href = decodeURIComponent(items[ref.getAttribute('idref')] || '');
    const data = files[base + href];
    if (data) parts.push(toMarkup(xml(data), base + href));
  }
  let text = parts.filter(Boolean).join('\n\n');
  // Pictures: scaled to the reader's stage, RGB565, after the text.
  const pics = new Pictures(pictureBudget()), ids = {};
  for (const m of [...text.matchAll(/\u0000IMG:([^\u0000]*)\u0000/g)]) {
    const path = m[1];
    if (path in ids) continue;
    ids[path] = '';
    const data = files[path];
    if (!data || pics.full()) continue;
    try {
      const bmp = await createImageBitmap(new Blob([data]));
      ids[path] = pics.add(bmp, 0, 0, bmp.width, bmp.height);
    } catch (e) { /* not a picture the browser can read */ }
  }
  text = text.replace(/\u0000IMG:([^\u0000]*)\u0000/g, (_, p) => ids[p] || '').replace(/\n{3,}/g, '\n\n');
  // Some "ebooks" are only photos of the printed pages (e.g. a scan run
  // through Calibre): nothing to read word by word.
  if ((text.replace(/\{image:\d+\}/g, '').match(/\p{L}/gu) || []).length < 500)
    throw new Error('this EPUB has only pictures of the pages, no text: use a PDF with a text layer (OCR) instead');
  return {title, author, text, images: pics.blob(), pictures: pics.list.length};
}
// ---- converters end

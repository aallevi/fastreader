#!/bin/sh
# Rebuilds the browser demo in docs/demo: the reader compiled to
# WebAssembly, plus the sample books. Serve docs/ to try it locally:
#   python3 -m http.server -d docs 8765  →  http://localhost:8765/demo/
set -e
# rustup's cargo: a Homebrew cargo would not see the wasm target.
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --release -p reader-web --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/reader_web.wasm docs/demo/fastreader.wasm
cp books/sample.md books/sonnet-18.md books/tour-lists-and-tables.md docs/demo/books/
# The phone page's EPUB/PDF converters, and pdf.js for PDFs.
sed -n '/---- converters begin/,/---- converters end/p' web/index.html > docs/demo/convert.js
mkdir -p docs/demo/pdfjs
for f in pdf.min.mjs pdf.worker.min.mjs; do gunzip -c "assets/web/$f.gz" > "docs/demo/pdfjs/$f"; done
cp assets/web/LICENSE-pdfjs.txt docs/demo/pdfjs/
ls -l docs/demo/fastreader.wasm

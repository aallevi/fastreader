#!/bin/sh
# Builds dist/FastReader-<version>.dmg: FastReader.app (the desktop reader,
# Apple silicon + Intel) next to an Applications shortcut, to drag across.
# The app keeps books, places and settings in
# ~/Library/Application Support/FastReader; the sample books come inside.
set -e
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"   # rustup's cargo (it has both targets)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p reader-sim --target "$t"
done
rm -rf dist/FastReader.app dist/stage
APP=dist/FastReader.app
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/books"
lipo -create -output "$APP/Contents/MacOS/FastReader" \
  target/aarch64-apple-darwin/release/reader-sim target/x86_64-apple-darwin/release/reader-sim
[ -f packaging/AppIcon.icns ] || python3 packaging/make_icon.py
cp packaging/AppIcon.icns "$APP/Contents/Resources/"
{ echo "<!-- title: The Quiet Machine -->"; cat books/sample.md; } > "$APP/Contents/Resources/books/sample.md"
cp books/sonnet-18.md books/tour-lists-and-tables.md "$APP/Contents/Resources/books/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>FastReader</string>
  <key>CFBundleDisplayName</key><string>Fast Reader</string>
  <key>CFBundleIdentifier</key><string>io.github.aallevi.fastreader</string>
  <key>CFBundleExecutable</key><string>FastReader</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.books</string>
</dict></plist>
PLIST
# Ad-hoc signature (no Apple Developer ID): the first launch needs
# right-click > Open, or System Settings > Privacy & Security > Open Anyway.
codesign --force --deep --sign - "$APP"
mkdir -p dist/stage
cp -R "$APP" dist/stage/
ln -s /Applications dist/stage/Applications
DMG="dist/FastReader-$VERSION.dmg"
rm -f "$DMG"
hdiutil create -volname "Fast Reader" -srcfolder dist/stage -ov -format UDZO "$DMG" >/dev/null
rm -rf dist/stage
ls -lh "$DMG"

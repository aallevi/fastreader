#!/bin/sh
# One release, everything together: tests, the Mac app (DMG) and the ESP32
# firmware (an update image for 0x10000 that keeps the reader's settings and
# books, and a full image for a first install at 0x0), a git tag and the
# GitHub release.
#   packaging/release.sh 0.2.0 notes.md
# notes.md: what changed, for people (the release page). Its "- " lines are
# what the Mac app's update notice lists, so put the changes as bullets.
# Run it when a set of changes has been checked, not after every commit.
set -e
cd "$(dirname "$0")/.."
VERSION="$1"; NOTES="$2"
[ -n "$VERSION" ] && [ -f "$NOTES" ] || { echo "usage: $0 <version> <notes.md>"; exit 1; }
export PATH="$HOME/.cargo/bin:$PATH"
[ -z "$(git status --porcelain)" ] || { echo "commit your changes first"; exit 1; }

# The version, in the workspace and the firmware, committed and tagged
# first: the builds then name themselves plainly "<version>" (an untagged or
# changed tree adds " +<commit>").
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml firmware/Cargo.toml
cargo test --release --workspace -q
git add Cargo.toml Cargo.lock firmware/Cargo.toml firmware/Cargo.lock
git commit -q -m "Release $VERSION"
git tag "v$VERSION"

# The Mac app.
packaging/make_dmg.sh

# The firmware: the normal build (web view off until chosen in Settings).
# The update image is the program alone (written at 0x10000: settings,
# places and books in the flash stay). The full image adds bootloader and
# partition table, without padding (a padded image would wipe the books).
( . "$HOME/export-esp.sh" && cd firmware && cargo build --release )
ELF=firmware/target/xtensa-esp32s3-none-elf/release/fastreader
espflash save-image --chip esp32s3 --flash-size 16mb "$ELF" "dist/fastreader-es3c28p-$VERSION-update.bin"
espflash save-image --chip esp32s3 --merge --skip-padding --flash-size 16mb --partition-table firmware/partitions.csv \
  "$ELF" "dist/fastreader-es3c28p-$VERSION-full.bin"

git push -q && git push -q --tags
gh release create "v$VERSION" "dist/FastReader-$VERSION.dmg" \
  "dist/fastreader-es3c28p-$VERSION-update.bin" "dist/fastreader-es3c28p-$VERSION-full.bin" \
  --repo aallevi/fastreader --title "Fast Reader $VERSION" --notes-file "$NOTES"
echo "released v$VERSION"

#!/bin/sh
# One release, everything together: tests, the Mac app (DMG) and the ESP32
# firmware (one image, written at 0x0), a git tag and the GitHub release.
#   packaging/release.sh 0.2.0 notes.md
# notes.md: what changed, for people (the release page). Run it when a set
# of changes has been checked, not after every commit.
set -e
cd "$(dirname "$0")/.."
VERSION="$1"; NOTES="$2"
[ -n "$VERSION" ] && [ -f "$NOTES" ] || { echo "usage: $0 <version> <notes.md>"; exit 1; }
export PATH="$HOME/.cargo/bin:$PATH"
[ -z "$(git status --porcelain)" ] || { echo "commit your changes first"; exit 1; }

# The version, in the workspace and the firmware.
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml firmware/Cargo.toml
cargo test --release --workspace -q

# The Mac app.
packaging/make_dmg.sh

# The firmware: the normal build (web view off until chosen in Settings),
# merged with bootloader and partition table into one image at 0x0.
( . "$HOME/export-esp.sh" && cd firmware && cargo build --release )
espflash save-image --chip esp32s3 --merge --flash-size 16mb --partition-table firmware/partitions.csv \
  firmware/target/xtensa-esp32s3-none-elf/release/fastreader "dist/fastreader-es3c28p-$VERSION.bin"

git add Cargo.toml Cargo.lock firmware/Cargo.toml firmware/Cargo.lock
git commit -q -m "Release $VERSION"
git tag "v$VERSION"
git push -q && git push -q --tags
gh release create "v$VERSION" "dist/FastReader-$VERSION.dmg" "dist/fastreader-es3c28p-$VERSION.bin" \
  --repo aallevi/fastreader --title "Fast Reader $VERSION" --notes-file "$NOTES"
echo "released v$VERSION"

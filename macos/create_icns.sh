#!/bin/bash
set -euo pipefail

INPUT="${1:?Usage: create_icns.sh <input.png> [output.icns]}"
OUTPUT="${2:-AppIcon.icns}"
ICONSET_DIR="$(mktemp -d)/AppIcon.iconset"

mkdir -p "$ICONSET_DIR"

sips -z 16 16     "$INPUT" --out "$ICONSET_DIR/icon_16x16.png"      >/dev/null
sips -z 32 32     "$INPUT" --out "$ICONSET_DIR/icon_16x16@2x.png"   >/dev/null
sips -z 32 32     "$INPUT" --out "$ICONSET_DIR/icon_32x32.png"      >/dev/null
sips -z 64 64     "$INPUT" --out "$ICONSET_DIR/icon_32x32@2x.png"   >/dev/null
sips -z 128 128   "$INPUT" --out "$ICONSET_DIR/icon_128x128.png"    >/dev/null
sips -z 256 256   "$INPUT" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
sips -z 256 256   "$INPUT" --out "$ICONSET_DIR/icon_256x256.png"    >/dev/null
sips -z 512 512   "$INPUT" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
sips -z 512 512   "$INPUT" --out "$ICONSET_DIR/icon_512x512.png"    >/dev/null
sips -z 1024 1024 "$INPUT" --out "$ICONSET_DIR/icon_512x512@2x.png" >/dev/null

iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT"
rm -rf "$ICONSET_DIR"

echo "Created $OUTPUT"

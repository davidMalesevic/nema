#!/usr/bin/env bash
# Baut ein Linux-AppImage für Nema.
#
# Voraussetzungen: cargo (Release-Build), appimagetool, eine libpdfium.so
# (z.B. via: curl -L -o pdfium.tgz https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-linux-x64.tgz)
#
# Aufruf:  ./packaging/build-appimage.sh [Pfad-zu-libpdfium.so] [appimagetool]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PDFIUM_SO="${1:-}"
APPIMAGETOOL="${2:-appimagetool}"

if [ -z "$PDFIUM_SO" ]; then
  echo "Usage: $0 <libpdfium.so> [appimagetool]"
  echo "libpdfium.so bekommst du z.B. so:"
  echo "  curl -L -o /tmp/pdfium.tgz https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-linux-x64.tgz"
  echo "  tar xzf /tmp/pdfium.tgz -C /tmp && cp /tmp/lib/libpdfium.so ."
  exit 1
fi

echo "== cargo build --release =="
cargo build --release -p nema-app --manifest-path "$ROOT/Cargo.toml"

APPDIR="$ROOT/dist/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" "$APPDIR/usr/share/icons/hicolor/256x256/apps"

cp "$ROOT/target/release/nema-app" "$APPDIR/usr/bin/nema-app"
cp "$PDFIUM_SO" "$APPDIR/usr/lib/libpdfium.so"
cp "$ROOT/packaging/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"
cp "$ROOT/packaging/nema.desktop" "$APPDIR/nema.desktop"
cp "$ROOT/packaging/nema.png" "$APPDIR/nema.png"
cp "$ROOT/packaging/nema.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/nema.png"
chmod -R a+rX "$APPDIR"
chmod a+rx "$APPDIR/AppRun"

mkdir -p "$ROOT/dist"
cd "$ROOT/dist"
echo "== appimagetool =="
# Im Container ohne FUSE: APPIMAGE_EXTRACT_AND_RUN=1
APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" "$APPDIR" "nema-x86_64.AppImage" || "$APPIMAGETOOL" "$APPDIR" "nema-x86_64.AppImage"
ls -la "$ROOT/dist/nema-x86_64.AppImage"
echo "Fertig: $ROOT/dist/nema-x86_64.AppImage"

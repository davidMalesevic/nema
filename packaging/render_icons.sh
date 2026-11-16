#!/usr/bin/env bash
# Rasterisiert die Logo-SVGs zu PNGs (braucht rsvg-convert).
# Ausgangspunkt sind die Original-SVGs (siehe nema-*.svg in diesem Ordner).
set -euo pipefail
cd "$(dirname "$0")"
for s in 256 512 1024; do
  rsvg-convert -w "$s" -h "$s" nema-icon.svg -o "nema${s/256/}.png"
done
rsvg-convert -w 1536 nema-logo.svg -o nema-logo.png
rsvg-convert -w 512 nema-favicon.svg -o nema-favicon.png
echo "Icons neu gerendert."

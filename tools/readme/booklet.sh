#!/bin/sh
# Draws docs/images/booklet.png: pages 1, 2 and 5 of the Compline example booklet, side by
# side. Needs EB Garamond 12 installed (fonts-ebgaramond), Poppler's pdftoppm and ImageMagick.
set -eu
cd "$(dirname "$0")/../.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cargo run -q --release -p neuma-cli -- book examples/compline/compline.book -o "$tmp/compline.pdf" 2>/dev/null
pdftoppm -r 110 -png "$tmp/compline.pdf" "$tmp/page"
for p in 1 2 5; do
  convert "$tmp/page-$p.png" -bordercolor '#d0d7de' -border 1 "$tmp/b$p.png"
done
convert -background none "$tmp/b1.png" "$tmp/b2.png" "$tmp/b5.png" -splice 28x0 +append -chop 28x0 +repage \
  -colors 128 -define png:compression-level=9 docs/images/booklet.png
echo "docs/images/booklet.png: $(wc -c < docs/images/booklet.png) bytes"

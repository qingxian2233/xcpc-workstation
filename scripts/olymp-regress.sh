#!/usr/bin/env bash
# Build every olymp fixture's workspace with `xcpc` (workspace → Markdown → Typst → PDF)
# and set each page beside its reference.
#
#   scripts/olymp-regress.sh [OUT_DIR] [fixture-name...]
#
# Checks that page counts match, then writes OUT_DIR/<fixture>-<page>.png
# (reference | ours | overlay) for review: the pages must look the same.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:-target/olymp-regress}
shift || true
names=("$@")
if [[ ${#names[@]} -eq 0 ]]; then
  for d in tests/fixtures/olymp-ref/*/; do names+=("$(basename "$d")"); done
fi
mkdir -p "$out"
cargo build -q -p xcpc-mcp
status=0
for n in "${names[@]}"; do
  dir=tests/fixtures/olymp-ref/$n
  ./target/debug/xcpc -w "$dir/workspace" --read-only build contest "$n" -o "$out/$n.pdf" 2>/dev/null
  ref_pages=$(pdfinfo "$dir/reference.pdf" | awk '/^Pages:/{print $2}')
  our_pages=$(pdfinfo "$out/$n.pdf" | awk '/^Pages:/{print $2}')
  if [[ $ref_pages != "$our_pages" ]]; then
    echo "$n: pages differ (reference $ref_pages, ours $our_pages)"
    status=1
  else
    echo "$n: $our_pages pages"
  fi
  pages=$(( ref_pages < our_pages ? ref_pages : our_pages ))
  for ((p = 1; p <= pages; p++)); do
    python3 scripts/side-by-side.py "$dir/reference.pdf" "$out/$n.pdf" "$p" "$out/$n-$p.png"
  done
done
echo "side-by-side pages in $out/"
exit $status

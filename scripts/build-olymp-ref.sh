#!/usr/bin/env bash
# Build the reference PDF of one olymp fixture with the pinned polygon-chinese-template
# (docs/statement-style.md §1.1). Run inside `nix develop`.
#
#   scripts/build-olymp-ref.sh tests/fixtures/olymp-ref/contest-zh
#
# Writes <case>/reference.pdf. The only change to the template is ctexart's
# `fontset=founder`, so that the reference uses the same FounderType fonts as our engine.
set -euo pipefail

case_dir=$(realpath "${1:?usage: $0 <fixture dir containing contest.md>}")
: "${XCPC_OLYMP_REFERENCE:?run inside nix develop}"
: "${XCPC_FONTCONFIG_FILE:?run inside nix develop}"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cp -r "$XCPC_OLYMP_REFERENCE"/. "$work"
chmod -R u+w "$work"
sed -i 's/\\documentclass\[12pt,a4paper,oneside\]{ctexart}/\\documentclass[12pt,a4paper,oneside,fontset=founder]{ctexart}/' \
  "$work"/pandoc/*.template.tex
grep -q 'fontset=founder' "$work/pandoc/contest.template.tex"

mkdir -p "$work/case"
cp -r "$case_dir"/. "$work/case"
rm -f "$work/case/reference.pdf"

cd "$work"
mkdir -p build/problems
pandoc \
  --from=markdown-implicit_figures+fenced_divs+fenced_code_attributes \
  --to=latex \
  --standalone \
  --template=pandoc/contest.template.tex \
  --lua-filter=pandoc/contest.lua \
  --metadata=problem-template:pandoc/problem.template.tex \
  --metadata=fragments-dir:build/problems \
  --output=build/statements.tex \
  case/contest.md

# Restrict fontconfig to the pinned fonts so the reference never picks up system fonts.
FONTCONFIG_FILE="$XCPC_FONTCONFIG_FILE" SOURCE_DATE_EPOCH=0 tectonic ${TECTONIC_FLAGS:-} \
  --chatter=minimal \
  -Z search-path=. \
  --outdir=build \
  build/statements.tex

cp build/statements.pdf "$case_dir/reference.pdf"
echo "wrote $case_dir/reference.pdf ($(pdfinfo build/statements.pdf | awk '/^Pages:/{print $2}') pages)"

#!/usr/bin/env bash
# Downloads the statement fonts into `.fonts/` (git-ignored) for machines without nix.
# Afterwards: export XCPC_FONT_DIRS="$PWD/.fonts"
#
# - Latin Modern (GUST Font License) and JetBrains Mono (OFL) are embedded in release builds.
# - CJK: Noto CJK SC (OFL); FounderType files can be added by hand to .fonts/founder/.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/.fonts"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$dir/lm" "$dir/jetbrains" "$dir/founder" "$dir/cjk"

fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }

if [ ! -f "$dir/lm/latinmodern-math.otf" ]; then
  echo "Latin Modern…"
  fetch https://www.gust.org.pl/projects/e-foundry/latin-modern/download/lm2.004otf.zip "$tmp/lm.zip"
  unzip -qjo "$tmp/lm.zip" '*.otf' -d "$dir/lm"
  fetch https://www.gust.org.pl/projects/e-foundry/lm-math/download/latinmodern-math-1959.zip "$tmp/lmm.zip"
  unzip -qjo "$tmp/lmm.zip" '*.otf' -d "$dir/lm"
fi

if [ ! -f "$dir/jetbrains/JetBrainsMono-Regular.ttf" ]; then
  echo "JetBrains Mono…"
  fetch https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip "$tmp/jb.zip"
  unzip -qjo "$tmp/jb.zip" 'fonts/ttf/JetBrainsMono-Regular.ttf' 'fonts/ttf/JetBrainsMono-Italic.ttf' \
    'fonts/ttf/JetBrainsMono-Bold.ttf' 'fonts/ttf/JetBrainsMono-BoldItalic.ttf' -d "$dir/jetbrains"
fi

# CJK: Noto Serif/Sans CJK SC (OFL). The template prefers FounderType (FZShuSong-Z01,
# FZHei-B01, …); if you hold a licence for them, drop the .TTF files into .fonts/founder/
# and they win automatically. They are never downloaded or redistributed here.
noto=https://github.com/notofonts/noto-cjk/raw/main
for f in Serif/OTF/SimplifiedChinese/NotoSerifCJKsc-Regular.otf Serif/OTF/SimplifiedChinese/NotoSerifCJKsc-Bold.otf \
         Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Bold.otf; do
  name="$(basename "$f")"
  if [ ! -f "$dir/cjk/$name" ]; then
    echo "$name…"
    mkdir -p "$dir/cjk"
    fetch "$noto/$f" "$dir/cjk/$name"
  fi
done

echo "done: export XCPC_FONT_DIRS=\"$dir\""

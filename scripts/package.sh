#!/usr/bin/env bash
# Package release binaries (desktop app via NSH + `xcpc` CLI) as archives.
# UPX compression is disabled by default for public releases (pass --upx to opt in).
# Run inside `nix develop` (or in CI with XCPC_FONT_DIRS set).
#
#   scripts/package.sh [--target <linux|windows|all>] [--out <DIR>] [--upx|--no-upx]
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/package.sh [OPTIONS]

Options:
  --target <linux|windows|all>  Target platform(s) to package [default: linux]
  --out <DIR>                   Output directory for archives [default: target/dist]
  --upx                         Enable UPX executable compression (off by default)
  --no-upx                      Disable UPX executable compression (default)
  -h, --help                    Show this help message
EOF
}

target="linux"
out_arg=""
use_upx=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      [[ $# -ge 2 ]] || { echo "error: --target requires a value" >&2; exit 2; }
      target="$2"
      shift 2
      ;;
    --out)
      [[ $# -ge 2 ]] || { echo "error: --out requires a value" >&2; exit 2; }
      out_arg="$2"
      shift 2
      ;;
    --upx)
      use_upx=1
      shift
      ;;
    --no-upx)
      use_upx=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$target" in
  linux|windows|all) ;;
  *)
    echo "error: invalid --target '$target' (expected linux, windows, or all)" >&2
    exit 2
    ;;
esac

repo_root=$(cd "$(dirname "$0")/.." && pwd)
if [[ -z "$out_arg" ]]; then
  out_dir="$repo_root/target/dist"
else
  out_dir=$(python3 -c 'import os, sys; print(os.path.abspath(sys.argv[1]))' "$out_arg")
fi

: "${XCPC_FONT_DIRS:?run inside nix develop or set XCPC_FONT_DIRS for bundled fonts}"

for cmd in nsh cargo python3 tar; do
  command -v "$cmd" >/dev/null || {
    echo "error: required tool '$cmd' not found in PATH (run inside nix develop)" >&2
    exit 1
  }
done

if [[ $use_upx -eq 1 ]]; then
  command -v upx >/dev/null || {
    echo "error: upx not found in PATH (omit --upx or run inside nix develop)" >&2
    exit 1
  }
fi

if [[ "$target" == "windows" || "$target" == "all" ]]; then
  for cmd in cargo-xwin clang-cl lld-link llvm-lib llvm-rc; do
    command -v "$cmd" >/dev/null || {
      echo "error: Windows cross-compile tool '$cmd' not found in PATH (run inside nix develop)" >&2
      exit 1
    }
  done
  win_libdir=$(rustc --print target-libdir --target x86_64-pc-windows-msvc 2>/dev/null || true)
  if [[ -z "$win_libdir" ]] || ! compgen -G "$win_libdir/libstd-*.rlib" >/dev/null; then
    echo "error: Rust target std for x86_64-pc-windows-msvc not found (run inside nix develop)" >&2
    exit 1
  fi
fi

app_version=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$repo_root/apps/desktop/nsh.json")
app_icon="$repo_root/assets/brand/icon-512.png"
[[ -f "$app_icon" ]] || { echo "error: brand icon not found: $app_icon" >&2; exit 1; }

mkdir -p "$out_dir"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

summary_file="$stage/summary.tsv"
: > "$summary_file"

cd "$repo_root"

compress_bin() {
  local bin=$1
  local platform=$2
  local raw_bytes after_bytes

  raw_bytes=$(stat -c %s "$bin")
  if [[ $use_upx -eq 1 ]]; then
    if [[ "$platform" == "windows" ]]; then
      upx -q --best --lzma "$bin" >/dev/null 2>&1 \
        || upx -q --best --lzma --compress-resources=0 "$bin" >/dev/null 2>&1 \
        || upx -q --best "$bin" >/dev/null 2>&1 \
        || upx -q --best --compress-resources=0 "$bin" >/dev/null
    else
      upx -q --best --lzma "$bin" >/dev/null 2>&1 \
        || upx -q --best "$bin" >/dev/null
    fi
    upx -t "$bin" >/dev/null
  fi
  after_bytes=$(stat -c %s "$bin")
  echo "$raw_bytes $after_bytes"
}

zip_dir() {
  local base_dir=$1
  local entry_name=$2
  local zip_path=$3
  python3 - "$base_dir" "$entry_name" "$zip_path" <<'PY'
import os
import sys
import zipfile

base_dir, entry_name, zip_path = sys.argv[1:4]
target = os.path.join(base_dir, entry_name)
with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
    if os.path.isfile(target):
        zf.write(target, entry_name)
    else:
        for root, _, files in os.walk(target):
            for name in sorted(files):
                full = os.path.join(root, name)
                rel = os.path.relpath(full, base_dir)
                zf.write(full, rel)
PY
}

record_summary() {
  local name=$1 raw=$2 upx_sz=$3 arch_sz=$4 arch_path=$5
  printf "%s\t%s\t%s\t%s\t%s\n" "$name" "$raw" "$upx_sz" "$arch_sz" "$arch_path" >> "$summary_file"
}

package_linux() {
  echo "==> Packaging Linux CLI (xcpc)..."
  cargo build --release -p xcpc-mcp
  local cli_stage="$stage/linux-cli"
  mkdir -p "$cli_stage"
  cp "$repo_root/target/release/xcpc" "$cli_stage/xcpc"
  chmod +x "$cli_stage/xcpc"
  read -r cli_raw cli_upx < <(compress_bin "$cli_stage/xcpc" linux)

  local cli_archive="$out_dir/xcpc-cli-${app_version}-linux-x86_64.tar.gz"
  GZIP=-9 tar -C "$cli_stage" -czf "$cli_archive" xcpc
  record_summary "cli (linux)" "$cli_raw" "$cli_upx" "$(stat -c %s "$cli_archive")" "$cli_archive"

  echo "==> Packaging Linux desktop..."
  local desk_stage="$stage/linux-desktop"
  mkdir -p "$desk_stage"
  # Do not pass --icon on Linux: NSH's hidden GTK titlebar draws a misaligned 16px
  # icon in the top-left corner when bundled; Linux launcher/taskbar icons come from
  # .desktop + hicolor icon sets installed by .deb / .pkg.tar.zst / AppImage.
  (cd "$repo_root/apps/desktop" && nsh package --target linux -o "$desk_stage")

  local app_dir
  app_dir=$(find "$desk_stage" -mindepth 1 -maxdepth 1 -type d | head -n 1)
  [[ -n "$app_dir" ]] || { echo "error: packaged Linux app directory not found in $desk_stage" >&2; exit 1; }
  local app_name
  app_name=$(basename "$app_dir")

  local desk_raw=0 desk_upx=0 found_bin=0
  while IFS= read -r -d '' bin; do
    if head -c 4 "$bin" | grep -q $'^\x7fELF'; then
      found_bin=1
      read -r r a < <(compress_bin "$bin" linux)
      desk_raw=$((desk_raw + r))
      desk_upx=$((desk_upx + a))
    fi
  done < <(find "$app_dir" -maxdepth 1 -type f -executable -print0)
  [[ $found_bin -eq 1 ]] || { echo "error: no ELF executable found in $app_dir" >&2; exit 1; }

  # Include xcpc CLI beside the desktop runner in the portable archive.
  cp "$cli_stage/xcpc" "$app_dir/xcpc"
  chmod +x "$app_dir/xcpc"

  local desk_archive="$out_dir/xcpc-workstation-${app_version}-linux-x86_64.tar.gz"
  GZIP=-9 tar -C "$desk_stage" -czf "$desk_archive" "$app_name"
  record_summary "desktop (linux)" "$desk_raw" "$desk_upx" "$(stat -c %s "$desk_archive")" "$desk_archive"
}

package_windows() {
  echo "==> Packaging Windows CLI (xcpc.exe)..."
  cargo xwin build --release -p xcpc-mcp --target x86_64-pc-windows-msvc
  local cli_stage="$stage/windows-cli"
  mkdir -p "$cli_stage"
  cp "$repo_root/target/x86_64-pc-windows-msvc/release/xcpc.exe" "$cli_stage/xcpc.exe"
  read -r cli_raw cli_upx < <(compress_bin "$cli_stage/xcpc.exe" windows)

  local cli_archive="$out_dir/xcpc-cli-${app_version}-windows-x86_64.zip"
  zip_dir "$cli_stage" "xcpc.exe" "$cli_archive"
  record_summary "cli (windows)" "$cli_raw" "$cli_upx" "$(stat -c %s "$cli_archive")" "$cli_archive"

  echo "==> Packaging Windows desktop..."
  local desk_stage="$stage/windows-desktop"
  mkdir -p "$desk_stage"
  (cd "$repo_root/apps/desktop" && nsh package --target windows --icon "$app_icon" -o "$desk_stage")

  local app_dir
  app_dir=$(find "$desk_stage" -mindepth 1 -maxdepth 1 -type d | head -n 1)
  [[ -n "$app_dir" ]] || { echo "error: packaged Windows app directory not found in $desk_stage" >&2; exit 1; }
  local app_name
  app_name=$(basename "$app_dir")

  local desk_raw=0 desk_upx=0 found_bin=0
  while IFS= read -r -d '' bin; do
    found_bin=1
    read -r r a < <(compress_bin "$bin" windows)
    desk_raw=$((desk_raw + r))
    desk_upx=$((desk_upx + a))
  done < <(find "$app_dir" -maxdepth 1 -type f -iname '*.exe' -print0)
  [[ $found_bin -eq 1 ]] || { echo "error: no .exe executable found in $app_dir" >&2; exit 1; }

  # Include xcpc.exe beside the desktop runner in the portable archive.
  cp "$cli_stage/xcpc.exe" "$app_dir/xcpc.exe"

  local desk_archive="$out_dir/xcpc-workstation-${app_version}-windows-x86_64.zip"
  zip_dir "$desk_stage" "$app_name" "$desk_archive"
  record_summary "desktop (windows)" "$desk_raw" "$desk_upx" "$(stat -c %s "$desk_archive")" "$desk_archive"
}

if [[ "$target" == "linux" || "$target" == "all" ]]; then
  package_linux
fi
if [[ "$target" == "windows" || "$target" == "all" ]]; then
  package_windows
fi

echo
python3 - "$summary_file" <<'PY'
import sys

rows = []
with open(sys.argv[1], "r", encoding="utf-8") as f:
    for line in f:
        line = line.rstrip("\n")
        if line:
            rows.append(line.split("\t"))

def fmt(n_str):
    n = int(n_str)
    return f"{n / (1024 * 1024):.2f} MiB ({n:,} B)"

def pct(after_str, raw_str):
    raw = int(raw_str)
    after = int(after_str)
    if raw == 0:
        return "0.0%"
    return f"{(1.0 - after / raw) * 100:.1f}%"

headers = ["Artifact", "Raw Binary", "After UPX", "Archive", "Bin Savings", "Path"]
table = []
for name, raw, upx_sz, arch, path in rows:
    table.append([name, fmt(raw), fmt(upx_sz), fmt(arch), pct(upx_sz, raw), path])

widths = [max(len(headers[i]), *(len(r[i]) for r in table)) for i in range(len(headers))]
sep = "  ".join("-" * w for w in widths)
print("  ".join(headers[i].ljust(widths[i]) for i in range(len(headers))))
print(sep)
for r in table:
    print("  ".join(r[i].ljust(widths[i]) for i in range(len(headers))))
PY

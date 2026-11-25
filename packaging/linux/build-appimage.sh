#!/usr/bin/env bash
# Build an x86_64 AppImage from a packaged Linux desktop directory and xcpc CLI binary.
#
# Usage: packaging/linux/build-appimage.sh <app_dir> <cli_bin> <version> <out_dir>
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "Usage: $0 <app_dir> <cli_bin> <version> <out_dir>" >&2
  exit 2
fi

app_dir=$1
cli_bin=$2
version=$3
out_dir=$4

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
[[ -d "$app_dir" ]] || { echo "error: app_dir not found: $app_dir" >&2; exit 1; }
[[ -f "$cli_bin" ]] || { echo "error: cli_bin not found: $cli_bin" >&2; exit 1; }

appimagetool_bin=${APPIMAGETOOL:-appimagetool}
command -v "$appimagetool_bin" >/dev/null || {
  echo "error: appimagetool not found in PATH (set APPIMAGETOOL or install appimagetool)" >&2
  exit 1
}

runner=$(find "$app_dir" -maxdepth 1 -type f -executable -print | head -n 1)
[[ -n "$runner" ]] || { echo "error: no executable runner found in $app_dir" >&2; exit 1; }
runner_name=$(basename "$runner")

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

appdir="$stage/XCPC-Workstation.AppDir"
install -dm755 "$appdir/usr/lib/xcpc-workstation" "$appdir/usr/bin"
cp -a "$app_dir/." "$appdir/usr/lib/xcpc-workstation/"
install -Dm755 "$cli_bin" "$appdir/usr/lib/xcpc-workstation/xcpc"
ln -s ../lib/xcpc-workstation/xcpc "$appdir/usr/bin/xcpc"

cat > "$appdir/AppRun" <<EOF
#!/usr/bin/env sh
HERE=\$(CDPATH= cd -- "\$(dirname -- "\$0")" && pwd)
if [ "\$(basename -- "\$0")" = "xcpc" ]; then
  exec "\$HERE/usr/lib/xcpc-workstation/xcpc" "\$@"
fi
exec "\$HERE/usr/lib/xcpc-workstation/$runner_name" "\$@"
EOF
chmod 755 "$appdir/AppRun"

install -Dm644 \
  "$repo_root/packaging/linux/dev.xcpc.workstation.desktop" \
  "$appdir/dev.xcpc.workstation.desktop"
install -Dm644 \
  "$repo_root/assets/brand/icon-256.png" \
  "$appdir/dev.xcpc.workstation.png"
ln -s dev.xcpc.workstation.png "$appdir/.DirIcon"

mkdir -p "$out_dir"
out_appimage="$out_dir/xcpc-workstation-${version}-linux-x86_64.AppImage"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$appimagetool_bin" "$appdir" "$out_appimage"
chmod +x "$out_appimage"
echo "Built $out_appimage"

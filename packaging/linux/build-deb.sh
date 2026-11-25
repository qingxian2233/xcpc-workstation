#!/usr/bin/env bash
# Build a Debian/Ubuntu .deb package from a packaged Linux desktop directory and xcpc CLI binary.
#
# Usage: packaging/linux/build-deb.sh <app_dir> <cli_bin> <version> <out_dir>
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
command -v dpkg-deb >/dev/null || { echo "error: dpkg-deb not found in PATH" >&2; exit 1; }

# Convert SemVer prerelease (e.g. 0.2.0-beta.1) to Debian version (0.2.0~beta.1)
# so prereleases sort before the final 0.2.0 release.
deb_version=${version/-/\~}

runner=$(find "$app_dir" -maxdepth 1 -type f -executable -print | head -n 1)
[[ -n "$runner" ]] || { echo "error: no executable runner found in $app_dir" >&2; exit 1; }
runner_name=$(basename "$runner")

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

root="$stage/pkg"
install -dm755 \
  "$root/DEBIAN" \
  "$root/opt/xcpc-workstation" \
  "$root/usr/bin" \
  "$root/usr/share/applications" \
  "$root/usr/share/doc/xcpc-workstation"

cp -a "$app_dir/." "$root/opt/xcpc-workstation/"
install -Dm755 "$cli_bin" "$root/opt/xcpc-workstation/xcpc"

cat > "$root/usr/bin/xcpc-workstation" <<EOF
#!/usr/bin/env sh
exec "/opt/xcpc-workstation/$runner_name" "\$@"
EOF
chmod 755 "$root/usr/bin/xcpc-workstation"
ln -s /opt/xcpc-workstation/xcpc "$root/usr/bin/xcpc"

install -Dm644 \
  "$repo_root/packaging/linux/dev.xcpc.workstation.desktop" \
  "$root/usr/share/applications/dev.xcpc.workstation.desktop"

for size in 16 24 32 48 64 128 256 512; do
  icon_src="$repo_root/assets/brand/icon-${size}.png"
  if [[ -f "$icon_src" ]]; then
    install -Dm644 "$icon_src" \
      "$root/usr/share/icons/hicolor/${size}x${size}/apps/dev.xcpc.workstation.png"
  fi
done

install -Dm644 \
  "$repo_root/assets/brand/icon.svg" \
  "$root/usr/share/icons/hicolor/scalable/apps/dev.xcpc.workstation.svg"
install -Dm644 \
  "$repo_root/assets/brand/xcpc-workstation-symbolic.svg" \
  "$root/usr/share/icons/hicolor/symbolic/apps/dev.xcpc.workstation-symbolic.svg"
install -Dm644 \
  "$repo_root/LICENSE" \
  "$root/usr/share/doc/xcpc-workstation/copyright"

installed_size=$(du -sk "$root/opt" "$root/usr" | awk '{s+=$1} END {print s}')

cat > "$root/DEBIAN/control" <<EOF
Package: xcpc-workstation
Version: ${deb_version}
Section: devel
Priority: optional
Architecture: amd64
Installed-Size: ${installed_size}
Depends: libgtk-4-1, libwebkitgtk-6.0-4, libgstreamer1.0-0, libgstreamer-plugins-base1.0-0, libc6
Maintainer: lihaoze123 <https://github.com/lihaoze123/xcpc-workstation>
Homepage: https://github.com/lihaoze123/xcpc-workstation
Description: Local-first personal XCPC workstation
 Problem bank, live-reference contest composition, olymp.sty-compatible
 Typst PDF publishing, and native CLI/MCP tooling for XCPC competitors
 and problem setters.
EOF

mkdir -p "$out_dir"
out_deb="$out_dir/xcpc-workstation_${deb_version}_amd64.deb"
dpkg-deb --root-owner-group --build -Zxz "$root" "$out_deb"
echo "Built $out_deb"

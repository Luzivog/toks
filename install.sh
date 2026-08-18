#!/usr/bin/env bash
# User-local install: binary, icons, and a launcher with a portable Exec path.
set -euo pipefail
cd "$(dirname "$0")"

tokscope_binary=target/release/tokscope
tokscope_prefix=${TOKSCOPE_INSTALL_PREFIX:-"$HOME/.local"}
tokscope_data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
tokscope_binary_dest="$tokscope_prefix/bin/tokscope"
tokscope_icon_root="$tokscope_data_home/icons/hicolor"
tokscope_app_root="$tokscope_data_home/applications"
tokscope_desktop_dest="$tokscope_app_root/tokscope.desktop"

if [[ ! -x "$tokscope_binary" ]]; then
    echo "release binary missing — run: cargo build --release --locked" >&2
    exit 1
fi

install -Dm755 "$tokscope_binary" "$tokscope_binary_dest"
install -Dm644 assets/tokscope.svg \
    "$tokscope_icon_root/scalable/apps/tokscope.svg"

# PNG sizes help shells that do not rasterize SVGs (ImageMagick is optional).
if command -v convert >/dev/null 2>&1; then
    for tokscope_icon_size in 48 64 128 256; do
        tokscope_icon_dir="$tokscope_icon_root/${tokscope_icon_size}x${tokscope_icon_size}/apps"
        mkdir -p "$tokscope_icon_dir"
        convert -background none -resize "${tokscope_icon_size}x${tokscope_icon_size}" \
            assets/tokscope.svg "$tokscope_icon_dir/tokscope.png"
    done
fi

tokscope_desktop_tmp=$(mktemp)
trap 'rm -f "$tokscope_desktop_tmp"' EXIT
awk -v executable="$tokscope_binary_dest" '
    /^Exec=/ { print "Exec=" executable; next }
    /^TryExec=/ { print "TryExec=" executable; next }
    { print }
' assets/tokscope.desktop >"$tokscope_desktop_tmp"
install -Dm644 "$tokscope_desktop_tmp" "$tokscope_desktop_dest"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$tokscope_desktop_dest"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$tokscope_app_root" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f "$tokscope_icon_root" 2>/dev/null || true
fi

echo "installed: $tokscope_binary_dest + $tokscope_desktop_dest"

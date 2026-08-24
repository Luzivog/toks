#!/usr/bin/env bash
# User-local install: binary, icons, and a launcher with a portable Exec path.
set -euo pipefail
cd "$(dirname "$0")"

toks_binary=target/release/toks
toks_router_binary=target/release/toks-router
toks_prefix=${TOKS_INSTALL_PREFIX:-${TOKSCOPE_INSTALL_PREFIX:-"$HOME/.local"}}
toks_data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
toks_binary_dest="$toks_prefix/bin/toks"
toks_router_binary_dest="$toks_prefix/bin/toks-router"
toks_router_artifact_root="$toks_data_home/toks/rotation/router-artifacts/executables"
toks_legacy_binary_dest="$toks_prefix/bin/tokscope"
toks_icon_root="$toks_data_home/icons/hicolor"
toks_app_root="$toks_data_home/applications"
toks_desktop_dest="$toks_app_root/toks.desktop"
toks_was_running=false
toks_router_artifact_temp=
toks_router_link_temp=
toks_desktop_tmp=

toks_cleanup() {
    [[ -z "$toks_desktop_tmp" ]] || rm -f "$toks_desktop_tmp"
    [[ -z "$toks_router_artifact_temp" ]] || rm -f "$toks_router_artifact_temp"
    [[ -z "$toks_router_link_temp" ]] || rm -f "$toks_router_link_temp"
}
trap toks_cleanup EXIT

source scripts/install/router-guards.sh

for toks_router_unit in toks-router.service toks-router.socket; do
    if toks_systemctl_present "$toks_router_unit"; then
        toks_probe_status=0
    else
        toks_probe_status=$?
    fi
    case $toks_probe_status in
        0 | 1) ;;
        *) exit 1 ;;
    esac
done

for toks_process in /proc/[0-9]*; do
    toks_process_command=$(
        tr '\0' '\n' 2>/dev/null <"$toks_process/cmdline" | head -n 1 || true
    )
    if [[ "$toks_process_command" == "$toks_binary_dest" ||
        "$toks_process_command" == "$toks_legacy_binary_dest" ]]; then
        toks_was_running=true
        break
    fi
done

if [[ ! -x "$toks_binary" || ! -x "$toks_router_binary" ]]; then
    echo "release binaries missing — build both Toks and toks-router first" >&2
    exit 1
fi

install -Dm755 "$toks_binary" "$toks_binary_dest"
toks_router_build=$(sha256sum "$toks_router_binary" | awk '{print $1}')
toks_router_artifact="$toks_router_artifact_root/$toks_router_build/toks-router"
toks_prepare_directory_under \
    "$toks_data_home" \
    "toks/rotation/router-artifacts/executables/$toks_router_build"
if [[ -L "$toks_router_artifact" || (-e "$toks_router_artifact" && ! -f "$toks_router_artifact") ]]; then
    echo "router artifact is not a regular file: $toks_router_artifact" >&2
    exit 1
elif [[ -e "$toks_router_artifact" ]]; then
    if ! cmp -s "$toks_router_binary" "$toks_router_artifact"; then
        echo "router artifact hash collision: $toks_router_artifact" >&2
        exit 1
    fi
else
    toks_router_artifact_temp=$(mktemp "$(dirname "$toks_router_artifact")/.toks-router.XXXXXX")
    install -m755 "$toks_router_binary" "$toks_router_artifact_temp"
    if ! ln "$toks_router_artifact_temp" "$toks_router_artifact" 2>/dev/null; then
        if [[ -L "$toks_router_artifact" || ! -f "$toks_router_artifact" ]] ||
            ! cmp -s "$toks_router_binary" "$toks_router_artifact"; then
            echo "router artifact hash collision: $toks_router_artifact" >&2
            exit 1
        fi
    fi
    rm -f "$toks_router_artifact_temp"
    toks_router_artifact_temp=
fi
chmod 755 "$toks_router_artifact"
mkdir -p "$(dirname "$toks_router_binary_dest")"
toks_router_link_temp=$(mktemp "$(dirname "$toks_router_binary_dest")/.toks-router.XXXXXX")
rm -f "$toks_router_link_temp"
ln -s "$toks_router_artifact" "$toks_router_link_temp"
mv -Tf "$toks_router_link_temp" "$toks_router_binary_dest"
install -Dm644 assets/toks.svg \
    "$toks_icon_root/scalable/apps/toks.svg"

# PNG sizes help shells that do not rasterize SVGs (ImageMagick is optional).
if command -v convert >/dev/null 2>&1; then
    for toks_icon_size in 48 64 128 256; do
        toks_icon_dir="$toks_icon_root/${toks_icon_size}x${toks_icon_size}/apps"
        mkdir -p "$toks_icon_dir"
        convert -background none -resize "${toks_icon_size}x${toks_icon_size}" \
            assets/toks.svg "$toks_icon_dir/toks.png"
    done
fi

toks_desktop_tmp=$(mktemp)
awk -v executable="$toks_binary_dest" '
    /^Exec=/ { print "Exec=" executable; next }
    /^TryExec=/ { print "TryExec=" executable; next }
    { print }
' assets/toks.desktop >"$toks_desktop_tmp"
install -Dm644 "$toks_desktop_tmp" "$toks_desktop_dest"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$toks_desktop_dest"
fi

# Remove only the obsolete product artifacts after the new install validates.
# Local usage history and account state live elsewhere and are never touched.
rm -f -- \
    "$toks_legacy_binary_dest" \
    "$toks_app_root/tokscope.desktop" \
    "$toks_icon_root/scalable/apps/tokscope.svg"
for toks_icon_size in 48 64 128 256; do
    rm -f -- "$toks_icon_root/${toks_icon_size}x${toks_icon_size}/apps/tokscope.png"
done

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$toks_app_root" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f "$toks_icon_root" 2>/dev/null || true
fi

"$toks_router_binary_dest" install-service "$toks_router_binary_dest"

echo "installed: $toks_binary_dest + $toks_router_binary_dest + $toks_desktop_dest"
if [[ "$toks_was_running" == true ]]; then
    echo "The previous app is still running — close it before opening Toks so local data can migrate safely."
fi

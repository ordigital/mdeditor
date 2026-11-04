#!/bin/sh
# Installs the launcher entry and icon for the current user, so that desktop environments
# (notably on Wayland, where the window icon comes from the .desktop file) show MDEdit's icon.
set -e
root=$(cd "$(dirname "$0")/.." && pwd)
share="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$share/applications" "$share/icons/hicolor/scalable/apps" "$share/icons/hicolor/256x256/apps"
cp "$root/assets/icon.svg" "$share/icons/hicolor/scalable/apps/mdedit.svg"
cp "$root/assets/icon.png" "$share/icons/hicolor/256x256/apps/mdedit.png"
sed "s|^Exec=.*|Exec=$root/target/release/mdedit %F|" "$root/packaging/mdedit.desktop" > "$share/applications/mdedit.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$share/icons/hicolor" 2>/dev/null || true
echo "Installed into $share"

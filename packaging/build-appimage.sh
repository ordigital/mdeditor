#!/bin/sh
# Builds releases/MDEdit-<version>-x86_64.AppImage.
#
# The binary is compiled inside an Ubuntu 22.04 container (glibc 2.35), so that the AppImage also runs
# on older distributions; building on a rolling distro would require its very new glibc.
# Needs docker and appimagetool. Set KEEP_BUILD=1 to keep releases/build (cargo target cache).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
work="$root/releases/build"
out="$root/releases/MDEdit-$version-x86_64.AppImage"
mkdir -p "$work/target"

docker run --rm \
    -v "$root":/src:ro -v "$work/target":/target -v mdedit-cargo:/root/.cargo -v mdedit-rustup:/root/.rustup \
    -e CARGO_TARGET_DIR=/target -e CARGO_TERM_COLOR=never \
    ubuntu:22.04 sh -eu -c '
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends ca-certificates curl build-essential pkg-config
        /root/.cargo/bin/cargo --version >/dev/null 2>&1 || curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain stable
        . /root/.cargo/env
        cd /src
        cargo build --release --locked
        chown -R '"$(id -u):$(id -g)"' /target
    '

appdir="$work/AppDir"
rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" "$appdir/usr/share/icons/hicolor/256x256/apps" \
    "$appdir/usr/share/icons/hicolor/scalable/apps"
cp "$work/target/release/mdedit" "$appdir/usr/bin/"
cp "$root/packaging/mdedit.desktop" "$appdir/mdedit.desktop"
cp "$root/packaging/mdedit.desktop" "$appdir/usr/share/applications/"
cp "$root/assets/icon.png" "$appdir/mdedit.png"
cp "$root/assets/icon.png" "$appdir/.DirIcon"
cp "$root/assets/icon.png" "$appdir/usr/share/icons/hicolor/256x256/apps/mdedit.png"
cp "$root/assets/icon.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/mdedit.svg"
ln -s usr/bin/mdedit "$appdir/AppRun"

ARCH=x86_64 appimagetool --no-appstream "$appdir" "$out"
[ "${KEEP_BUILD:-0}" = 1 ] || rm -rf "$work"
echo "Built $out"

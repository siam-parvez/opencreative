#!/bin/sh
# Builds OpenCreative-<version>-<arch>.AppImage from release binaries.
# Usage: packaging/linux/build-appimage.sh <dir with opencreative and
#        opencreative-gui> <x86_64|aarch64> <output file>
# Needs `appimagetool` on PATH.
set -eu
bins="$1"
arch="$2"
out="$3"
here="$(dirname "$(readlink -f "$0")")"
appdir="$(mktemp -d)/OpenCreative.AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/metainfo"
install -m 755 "$bins/opencreative" "$bins/opencreative-gui" "$appdir/usr/bin/"
install -m 755 "$here/AppRun" "$appdir/AppRun"
# The packaged entry: uninstalling an AppImage goes through the app itself.
install -m 644 "$here/opencreative.desktop" "$appdir/opencreative.desktop"
install -m 644 "$here/opencreative.png" "$appdir/opencreative.png"
install -m 644 "$here/io.github.siam_parvez.opencreative.metainfo.xml" \
    "$appdir/usr/share/metainfo/opencreative.appdata.xml"
ARCH="$arch" appimagetool --no-appstream "$appdir" "$out"

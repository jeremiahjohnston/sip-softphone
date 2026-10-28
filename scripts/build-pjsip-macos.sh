#!/usr/bin/env bash
# Downloads and builds PJSIP for macOS into deps/pjsip (install prefix).
# Needs: Xcode command line tools (xcode-select --install).
set -euo pipefail
VERSION=2.15.1
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/deps/pjproject"
PREFIX="$ROOT/deps/pjsip"

mkdir -p "$ROOT/deps"
[ -d "$SRC" ] || git clone --depth 1 --branch "$VERSION" https://github.com/pjsip/pjproject "$SRC"
cd "$SRC"
# Our fixes on top of the release (see scripts/pjsip-patches/README.md).
for p in "$ROOT"/scripts/pjsip-patches/*.patch; do
  git apply --reverse --check "$p" 2>/dev/null || git apply "$p"
done
echo '#define PJMEDIA_HAS_VIDEO 0' > pjlib/include/pj/config_site.h
./configure --prefix="$PREFIX" --disable-video --disable-ssl --disable-libwebrtc \
  --disable-libyuv --disable-sdl --disable-ffmpeg --disable-openh264 --disable-vpx \
  CFLAGS="-O2 -fPIC"
make dep
make -j"$(sysctl -n hw.ncpu)"
make install
echo
echo "PJSIP built. Now run: scripts/run-macos.sh"

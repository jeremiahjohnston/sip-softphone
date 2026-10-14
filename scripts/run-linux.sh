#!/usr/bin/env bash
# Builds and starts the app (debug build). Run build-pjsip-linux.sh once first.
# Needs (Debian/Ubuntu): libwebkit2gtk-4.1-dev librsvg2-dev and Rust.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PJSIP_DIR="$ROOT/deps/pjsip"
cd "$ROOT"
cargo run -p sip-softphone "$@"

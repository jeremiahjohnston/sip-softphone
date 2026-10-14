#!/usr/bin/env bash
# Builds and starts the app (debug build). Run build-pjsip-macos.sh once first.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PJSIP_DIR="$ROOT/deps/pjsip"
cd "$ROOT"
cargo run -p sip-softphone "$@"

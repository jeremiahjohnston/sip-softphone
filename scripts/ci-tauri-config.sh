#!/usr/bin/env bash
# Writes the Tauri config overrides for a CI build to $1: a version number that
# goes up with every run, the release feed the app checks for updates, and
# whether to sign the update files (only when the signing key secret is set).
set -euo pipefail
version="0.2.${GITHUB_RUN_NUMBER:-0}"
repo="${RELEASE_REPO:-${GITHUB_REPOSITORY:-jeremiahjohnston/sip-softphone}}"
sign=false
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && sign=true
cat > "$1" <<JSON
{
  "version": "$version",
  "bundle": { "createUpdaterArtifacts": $sign },
  "plugins": {
    "updater": {
      "endpoints": ["https://github.com/$repo/releases/latest/download/latest.json"]
    }
  }
}
JSON
echo "Building $version, updates from $repo, signed: $sign"

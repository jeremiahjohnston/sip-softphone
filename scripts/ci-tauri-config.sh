#!/usr/bin/env bash
# Writes the Tauri config overrides for a CI build to $1: a version number that
# goes up with every run, the release feed the app checks for updates, and
# whether to sign the update files (only when the signing key secret is set).
# On macOS, with an Apple Developer ID certificate in APPLE_CERTIFICATE, the app
# is signed with it instead of the ad-hoc signature, so it can be notarized.
set -euo pipefail
version="0.2.${GITHUB_RUN_NUMBER:-0}"
repo="${RELEASE_REPO:-${GITHUB_REPOSITORY:-jeremiahjohnston/sip-softphone}}"
sign=false
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && sign=true
macos=""
if [ -n "${APPLE_CERTIFICATE:-}" ]; then
  # codesign accepts part of the certificate's name, and the build keychain
  # holds only this one certificate.
  identity="${APPLE_SIGNING_IDENTITY:-Developer ID Application}"
  macos=", \"macOS\": { \"signingIdentity\": \"$identity\" }"
fi
cat > "$1" <<JSON
{
  "version": "$version",
  "bundle": { "createUpdaterArtifacts": $sign$macos },
  "plugins": {
    "updater": {
      "endpoints": ["https://github.com/$repo/releases/latest/download/latest.json"]
    }
  }
}
JSON
echo "Building $version, updates from $repo, signed: $sign, Developer ID: ${identity:-none}"

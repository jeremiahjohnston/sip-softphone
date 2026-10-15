#!/usr/bin/env bash
# Installs or updates SIP Softphone on macOS from the latest GitHub Release:
#   curl -fsSL https://github.com/jeremiahjohnston/sip-softphone/releases/latest/download/install-macos.sh | bash
# Files downloaded with curl aren't quarantined, so macOS opens the app without
# the "damaged" warning it shows for an unnotarized app downloaded in a browser.
set -euo pipefail
repo="${SIP_SOFTPHONE_REPO:-jeremiahjohnston/sip-softphone}"
app="SIP Softphone.app"

[ "$(uname -s)" = Darwin ] || { echo "This installer is for macOS." >&2; exit 1; }
[ "$(uname -m)" = arm64 ] || { echo "Only Apple silicon Macs are supported for now." >&2; exit 1; }

dest=/Applications
[ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }

echo "Finding the latest release of $repo..."
feed=$(curl -fsSL "https://github.com/$repo/releases/latest/download/latest.json")
url=$(printf '%s' "$feed" | plutil -extract platforms.darwin-aarch64.url raw -o - -)
version=$(printf '%s' "$feed" | plutil -extract version raw -o - -)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading SIP Softphone $version..."
curl -fL --progress-bar -o "$tmp/app.tar.gz" "$url"
tar -xzf "$tmp/app.tar.gz" -C "$tmp"
[ -d "$tmp/$app" ] || { echo "The download didn't contain $app." >&2; exit 1; }

if pgrep -xq sip-softphone; then
  echo "Closing SIP Softphone..."
  osascript -e 'quit app "SIP Softphone"' >/dev/null 2>&1 || true
  sleep 2
fi
rm -rf "${dest:?}/$app"
mv "$tmp/$app" "$dest/"
# In case an older copy was installed from the browser download.
xattr -dr com.apple.quarantine "$dest/$app" 2>/dev/null || true

echo "Installed SIP Softphone $version in $dest. Opening it..."
open "$dest/$app"

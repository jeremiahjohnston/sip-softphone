#!/usr/bin/env bash
# Publishes a GitHub Release from the CI build artifacts in $1 and writes the
# latest.json the app's updater reads. Needs VERSION, REPO and GH_TOKEN.
set -euo pipefail
dir="$1"
tag="v$VERSION"
base="https://github.com/$REPO/releases/download/$tag"
out=$(mktemp -d)

# GitHub turns spaces in asset names into dots, so give the files plain names.
copy() { # source-file new-name
  cp "$1" "$out/$2"
  [ -f "$1.sig" ] && cp "$1.sig" "$out/$2.sig"
  echo "$2"
}
one() { find "$dir" -type f -name "$1" | head -1; }

mac_dmg=$(one '*.dmg')
mac_tgz=$(one '*.app.tar.gz')
win_exe=$(one '*-setup.exe')
deb=$(one '*.deb')
appimage=$(one '*.AppImage')

entry() { # name-in-release
  [ -f "$out/$1.sig" ] || return 0
  jq -n --arg sig "$(cat "$out/$1.sig")" --arg url "$base/$1" '{signature: $sig, url: $url}'
}

platforms='{}'
add() { # platform-key file-name
  local e; e=$(entry "$2")
  [ -n "$e" ] && platforms=$(jq --arg k "$1" --argjson v "$e" '. + {($k): $v}' <<<"$platforms")
  return 0
}

[ -n "$mac_dmg" ] && copy "$mac_dmg" "SIP-Softphone_${VERSION}_aarch64.dmg"
if [ -n "$mac_tgz" ]; then
  n=$(copy "$mac_tgz" "SIP-Softphone_${VERSION}_aarch64.app.tar.gz")
  add darwin-aarch64 "$n"
fi
if [ -n "$win_exe" ]; then
  n=$(copy "$win_exe" "SIP-Softphone_${VERSION}_x64-setup.exe")
  add windows-x86_64 "$n"
fi
if [ -n "$appimage" ]; then
  n=$(copy "$appimage" "SIP-Softphone_${VERSION}_amd64.AppImage")
  add linux-x86_64 "$n"
  add linux-x86_64-appimage "$n"
fi
if [ -n "$deb" ]; then
  n=$(copy "$deb" "SIP-Softphone_${VERSION}_amd64.deb")
  add linux-x86_64-deb "$n"
fi

[ "$platforms" != '{}' ] || { echo "No signed update files found" >&2; exit 1; }

# The macOS install command downloads this from the latest release.
sed "s|jeremiahjohnston/sip-softphone|$REPO|g" "$(dirname "$0")/install-macos.sh" > "$out/install-macos.sh"

notes="${NOTES:-}"
jq -n --arg v "$VERSION" --arg notes "$notes" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson p "$platforms" '{version: $v, notes: $notes, pub_date: $date, platforms: $p}' > "$out/latest.json"
cat "$out/latest.json"

target=()
[ "$REPO" = "${GITHUB_REPOSITORY:-}" ] && target=(--target "$GITHUB_SHA")
gh release create "$tag" --repo "$REPO" "${target[@]}" --title "SIP Softphone $VERSION" \
  --notes "$notes" "$out"/*

# SIP Softphone

A simple, modern SIP softphone that registers straight to your own PBX
(FreePBX / Asterisk). No vendor cloud, no account sign-up.

Built with [Tauri](https://tauri.app) (Rust + web UI) on top of
[PJSIP](https://www.pjsip.org). Runs on macOS, Windows and Linux.

## Status

- Registers one account; makes and receives calls
- Mute, hold, DTMF keypad
- Blind transfer ("Transfer now") and attended transfer ("Ask first", then Connect)
- Call waiting: answer a second call, swap between them
- Recents (call history with missed-call badge) and contacts
- Seven ringtones on the selected speaker; the window comes forward on incoming calls
- Tray / menu bar icon with a missed-call dot; closing the window keeps the phone running
- Voicemail indicator: a badge with the new-message count from the PBX (SIP MWI); click it, or "Call voicemail" in the tray, to dial voicemail (*97 unless you set another number)
- Do Not Disturb (header moon or tray menu) and call forwarding; turned-away calls still show in Recents
- Password saved in the system keychain; signs in automatically at launch
- Click-to-dial: tel:, sip: and callto: links open the app and dial
- Auto-answer for intercom/paging calls (or all calls)
- BLF: contacts that are extensions show free / ringing / busy
- Optional global hotkeys (answer, hang up, mute) and open at login
- Microphone and speaker selection
- Updates itself: checks for new versions at startup and every few hours; Settings > Updates installs one and restarts

## Layout

| Path | What |
| --- | --- |
| `sipcore/` | Rust wrapper around PJSIP. `csrc/shim.c` is a small C layer over `pjsua`, `src/lib.rs` the safe Rust API. |
| `src-tauri/` | The desktop app: Tauri commands that call `sipcore`, and SIP events forwarded to the UI. |
| `ui/` | Plain HTML/CSS/JS front end (no build step). Opening `ui/index.html` in a browser shows a mock preview. |
| `scripts/` | Build PJSIP and run the app on each OS. |

## Downloads

Every push to `main` builds a Windows installer, a macOS disk image, and a Linux
.deb and AppImage on GitHub Actions and publishes them as a GitHub Release
(version `0.2.<run number>`). Install once from the Releases page; after that
the app finds new releases itself and installs them in place from
**Settings > Updates**. Builds from other branches are under **Artifacts** on
the Actions run.

On macOS the app is not notarized yet. If macOS says it's damaged on the first
install, drag it to Applications and run
`xattr -cr "/Applications/SIP Softphone.app"` once. Updates installed from
inside the app don't need this.

### Release setup

Releases need two things in the repository settings:

1. The secret `TAURI_SIGNING_PRIVATE_KEY`: the private key matching the
   `pubkey` in `src-tauri/tauri.conf.json`. The app only installs updates
   signed with it. Without the secret, builds still run but nothing is
   published. (`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` too, if the key has one.)
2. Somewhere the app can download from without signing in: the updater fetches
   `https://github.com/<repo>/releases/latest/download/latest.json`, which only
   works for a public repository. To keep this repository private, create a
   public one for downloads, set the variable `RELEASE_REPO` to it
   (`owner/name`) and the secret `RELEASE_TOKEN` to a token that can create
   releases there.

## Building it yourself

### Windows

One-time setup:

1. Install [Git](https://git-scm.com/download/win).
2. Install [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++**.
3. Install Rust from [rustup.rs](https://rustup.rs) (default MSVC toolchain).
4. WebView2 is already on Windows 10/11.

Then, from this folder in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-pjsip-windows.ps1   # once
powershell -ExecutionPolicy Bypass -File scripts\run-windows.ps1
```

### Linux (Debian/Ubuntu)

```bash
sudo apt install build-essential git pkg-config libasound2-dev libwebkit2gtk-4.1-dev librsvg2-dev libdbus-1-dev
curl https://sh.rustup.rs -sSf | sh    # once
scripts/build-pjsip-linux.sh           # once
scripts/run-linux.sh
```

### macOS

```bash
xcode-select --install                 # once
curl https://sh.rustup.rs -sSf | sh    # once
scripts/build-pjsip-macos.sh           # once
scripts/run-macos.sh
```

macOS will ask for microphone access on the first call.

## Testing without the UI

`sipcore` has a command-line smoke test:

```bash
PJSIP_DIR=... SIP_SERVER=pbx.lan SIP_USER=1001 SIP_PASS=secret SIP_DIAL=1002 \
  cargo run -p sipcore --example smoke
```

See the top of `sipcore/examples/smoke.rs` for the other options.

## FreePBX notes

- Use a **PJSIP** extension (Chan SIP also works). UDP on port 5060 is the usual default.
- If you can call out but hear nothing, or calls drop after ~30 seconds, it's
  usually NAT or firewall on the RTP ports (10000-20000 on Asterisk by default).

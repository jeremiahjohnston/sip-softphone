# SIP Softphone

A simple, modern SIP softphone that works with any standard SIP server or
provider: FreePBX, Asterisk, 3CX, FreeSWITCH, FusionPBX, a hosted VoIP
service, and so on. No vendor cloud, no account sign-up.

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
- Voicemail indicator: a badge with the new-message count from the PBX (SIP MWI); click it, or "Call voicemail" in the tray, to dial voicemail (set your system's voicemail number in Account; *97 is used if you leave it blank)
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

On macOS, builds are only notarized once the Apple secrets under
[macOS signing](#macos-signing) are set. Until then macOS says the app is
damaged on the first install: drag it to Applications and run
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

### macOS signing

macOS only opens a downloaded app without warnings if it is signed with an
Apple Developer ID and notarized by Apple. That needs a paid Apple Developer
account (99 USD a year). Once the five secrets below are set, the macOS build
signs and notarizes the app itself, and the run's "macOS app" note ends with
`Gatekeeper: ... accepted ... source=Notarized Developer ID`.

1. **Join the Apple Developer Program.** Go to
   <https://developer.apple.com/programs/enroll/>, click **Start your
   enrollment**, sign in with your Apple Account, choose **Individual / Sole
   Proprietor** and pay. Approval can take a day or two.
2. **Make a certificate request on your Mac.** Open **Keychain Access**
   (Spotlight: "Keychain Access"). In the menu bar choose **Keychain Access >
   Certificate Assistant > Request a Certificate From a Certificate
   Authority...**. Enter your email and name, leave CA Email empty, pick
   **Saved to disk**, click **Continue** and save the file to the Desktop.
3. **Create the Developer ID certificate.** Go to
   <https://developer.apple.com/account/resources/certificates/list>, click
   **+**, choose **Developer ID Application**, click **Continue**, keep the
   **G2 Sub-CA** profile, upload the file from step 2, click **Continue**, then
   **Download**. Double-click the downloaded `.cer` file to add it to Keychain
   Access.
4. **Export it with its private key.** In Keychain Access select the **login**
   keychain and the **My Certificates** tab, right-click
   **Developer ID Application: <your name> (<team id>)** and choose
   **Export...**. Save as `Certificates.p12` on the Desktop and choose a
   password (this is `APPLE_CERTIFICATE_PASSWORD`).
5. **Copy the certificate as text.** In Terminal run
   `base64 -i ~/Desktop/Certificates.p12 | pbcopy`. The clipboard now holds
   `APPLE_CERTIFICATE`.
6. **Make an app-specific password** for notarization: at
   <https://account.apple.com> go to **Sign-In and Security > App-Specific
   Passwords**, click **+**, name it "sip-softphone" and copy the password
   (this is `APPLE_PASSWORD`).
7. **Find your Team ID** at <https://developer.apple.com/account> under
   **Membership details** (ten letters and digits; this is `APPLE_TEAM_ID`).
8. **Add the secrets.** On GitHub open this repository's **Settings > Secrets
   and variables > Actions** and click **New repository secret** for each:

   | Name | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | the text from step 5 |
   | `APPLE_CERTIFICATE_PASSWORD` | the password from step 4 |
   | `APPLE_ID` | your Apple Account email |
   | `APPLE_PASSWORD` | the app-specific password from step 6 |
   | `APPLE_TEAM_ID` | the Team ID from step 7 |

   `APPLE_SIGNING_IDENTITY` is optional: the full certificate name, only needed
   if the build picks the wrong one.
9. Delete `Certificates.p12` and the request file from the Desktop, then push
   to `main` (or re-run the last build) to publish a notarized release.

The first update after switching to the Developer ID may ask for microphone
access once more, because macOS ties that permission to the app's signature.

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

## Connecting to your phone system

- Enter the server, extension (or username) and password your phone system or
  provider gave you. UDP on port 5060 is the usual default; TCP and TLS are
  also available.
- On FreePBX, use a **PJSIP** extension (Chan SIP also works).
- If you can call out but hear nothing, or calls drop after ~30 seconds, it's
  usually NAT or a firewall blocking the audio (RTP) ports, 10000-20000 on
  Asterisk-based systems by default.

## License

The code in this repository is under the [MIT License](LICENSE). The app links
[PJSIP](https://www.pjsip.org), which is licensed under the GNU GPL v2, so the
built installers as a whole are distributed under the GPL v2; the full source
for them is this repository plus PJSIP's.

# SIP Softphone

A simple, modern SIP softphone that registers straight to your own PBX
(FreePBX / Asterisk). No vendor cloud, no account sign-up.

Built with [Tauri](https://tauri.app) (Rust + web UI) on top of
[PJSIP](https://www.pjsip.org). macOS and Windows today, Linux later.

## Status

Step 1: register one account, make and receive calls, mute, hold, DTMF keypad.

## Layout

| Path | What |
| --- | --- |
| `sipcore/` | Rust wrapper around PJSIP. `csrc/shim.c` is a small C layer over `pjsua`, `src/lib.rs` the safe Rust API. |
| `src-tauri/` | The desktop app: Tauri commands that call `sipcore`, and SIP events forwarded to the UI. |
| `ui/` | Plain HTML/CSS/JS front end (no build step). Opening `ui/index.html` in a browser shows a mock preview. |
| `scripts/` | Build PJSIP and run the app on each OS. |

## Windows

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

## macOS

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

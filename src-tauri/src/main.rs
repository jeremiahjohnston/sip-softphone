// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Deserialize;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use sipcore::{Account, SipEvent, Transport};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, UserAttentionType, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

/// Passed by the login item so the phone starts in the tray.
const MINIMIZED_ARG: &str = "--minimized";
/// Keychain / Credential Manager / Secret Service entry name.
const KEYRING_SERVICE: &str = "dev.jeremiah.sipsoftphone";

const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");
const TRAY_ICON_MISSED: &[u8] = include_bytes!("../icons/tray-missed.png");

#[derive(Deserialize)]
struct AccountForm {
    server: String,
    port: Option<u16>,
    transport: String,
    user: String,
    auth_user: Option<String>,
    password: String,
    display_name: Option<String>,
}

/// The current session's SIP log, rewritten each launch.
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOG: OnceLock<Mutex<File>> = OnceLock::new();

fn open_log(dir: PathBuf) {
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("sip.log");
    if let Ok(file) = File::create(&path) {
        let _ = LOG.set(Mutex::new(file));
        let _ = LOG_PATH.set(path);
    }
}

fn log_line(line: &str) {
    if let Some(Ok(mut f)) = LOG.get().map(|m| m.lock()) {
        let _ = writeln!(f, "{line}");
    }
}

fn write_log(ev: &SipEvent) {
    match ev {
        SipEvent::Log { line, .. } => log_line(line),
        other => log_line(&format!(">> {}", serde_json::to_string(other).unwrap_or_default())),
    }
}

/// Opens the folder holding the log with the file selected.
#[tauri::command]
fn show_log() -> Result<String, String> {
    let path = LOG_PATH.get().ok_or("No log file")?;
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    r.map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn err(e: sipcore::Error) -> String {
    e.to_string()
}

#[tauri::command]
fn sip_register(account: AccountForm) -> Result<(), String> {
    let transport = match account.transport.as_str() {
        "tcp" => Transport::Tcp,
        "tls" => Transport::Tls,
        _ => Transport::Udp,
    };
    sipcore::set_account(&Account {
        server: account.server.trim().to_string(),
        port: account.port.unwrap_or(0),
        transport,
        user: account.user.trim().to_string(),
        auth_user: account.auth_user.unwrap_or_default().trim().to_string(),
        password: account.password,
        display_name: account.display_name.unwrap_or_default(),
    })
    .map_err(err)
}

#[tauri::command]
fn sip_unregister() -> Result<(), String> {
    sipcore::remove_account().map_err(err)
}

#[tauri::command]
fn sip_call(dest: String) -> Result<i32, String> {
    sipcore::call(&dest).map_err(err)
}

#[tauri::command]
fn sip_answer(call_id: i32) -> Result<(), String> {
    sipcore::answer(call_id).map_err(err)
}

#[tauri::command]
fn sip_hangup(call_id: i32) -> Result<(), String> {
    sipcore::hangup(call_id).map_err(err)
}

#[tauri::command]
fn sip_dtmf(call_id: i32, digits: String) -> Result<(), String> {
    sipcore::send_dtmf(call_id, &digits).map_err(err)
}

#[tauri::command]
fn sip_mute(call_id: i32, mute: bool) -> Result<(), String> {
    sipcore::set_mute(call_id, mute).map_err(err)
}

#[tauri::command]
fn sip_hold(call_id: i32, hold: bool) -> Result<(), String> {
    sipcore::set_hold(call_id, hold).map_err(err)
}

#[tauri::command]
fn sip_transfer(call_id: i32, dest: String) -> Result<(), String> {
    sipcore::transfer(call_id, &dest).map_err(err)
}

#[tauri::command]
fn sip_transfer_attended(call_id: i32, dest_call_id: i32) -> Result<(), String> {
    sipcore::transfer_attended(call_id, dest_call_id).map_err(err)
}

#[tauri::command]
fn sip_ring(mode: sipcore::Ring, tone: Option<usize>) -> Result<(), String> {
    sipcore::ring(mode, tone.unwrap_or(0)).map_err(err)
}

#[tauri::command]
fn ringtones() -> Vec<&'static str> {
    sipcore::RINGTONES.to_vec()
}

#[tauri::command]
fn sip_set_dnd(app: AppHandle, on: bool) {
    sipcore::set_dnd(on);
    if let Some(item) = DND_ITEM.get() {
        let _ = item.set_checked(on);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_title(if on { Some("DND") } else { None::<&str> });
    }
}

#[tauri::command]
fn sip_set_forward(dest: Option<String>) {
    sipcore::set_forward(dest.as_deref());
}

#[tauri::command]
fn sip_watch(numbers: Vec<String>) -> Result<(), String> {
    sipcore::watch(&numbers).map_err(err)
}

// ---------- saved password ----------

fn keyring_entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())
}

/// The saved password for `account` ("user@server"), if any.
#[tauri::command]
fn password_get(account: String) -> Option<String> {
    keyring_entry(&account).ok()?.get_password().ok()
}

#[tauri::command]
fn password_set(account: String, password: String) -> Result<(), String> {
    keyring_entry(&account)?.set_password(&password).map_err(|e| e.to_string())
}

#[tauri::command]
fn password_delete(account: String) {
    if let Ok(entry) = keyring_entry(&account) {
        let _ = entry.delete_credential();
    }
}

// ---------- click-to-dial ----------

/// The last tel:/sip:/callto: link opened with the app, waiting for the UI.
static PENDING_DIAL: Mutex<Option<String>> = Mutex::new(None);

fn on_dial_links(app: &AppHandle, urls: Vec<String>) {
    let Some(url) = urls.into_iter().find(|u| {
        let u = u.to_ascii_lowercase();
        u.starts_with("tel:") || u.starts_with("sip:") || u.starts_with("sips:") || u.starts_with("callto:")
    }) else {
        return;
    };
    log_line(&format!("open link {url}"));
    if let Ok(mut p) = PENDING_DIAL.lock() {
        *p = Some(url);
    }
    show_main(app);
    let _ = app.emit("dial-link", ());
}

/// Hands the UI the pending link once, so a link that arrives before the
/// window has loaded still gets dialed.
#[tauri::command]
fn take_dial_link() -> Option<String> {
    PENDING_DIAL.lock().ok()?.take()
}

// ---------- hotkeys and login item ----------

/// Replaces the global hotkeys. Each entry is (action, accelerator), e.g.
/// ("answer", "CommandOrControl+Alt+Shift+A"). Pressing one emits "hotkey"
/// with the action name.
#[tauri::command]
fn set_hotkeys(app: AppHandle, keys: Vec<(String, String)>) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    let mut failed = Vec::new();
    for (action, accel) in keys {
        if accel.trim().is_empty() {
            continue;
        }
        let r = gs.on_shortcut(accel.trim(), move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = app.emit("hotkey", action.clone());
            }
        });
        if let Err(e) = r {
            log_line(&format!("hotkey {accel}: {e}"));
            failed.push(accel);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join(", "))
    }
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> Result<(), String> {
    let a = app.autolaunch();
    if on { a.enable() } else { a.disable() }.map_err(|e| e.to_string())
}

// ---------- tray / menu bar icon ----------

/// The tray menu's "missed calls" and "voicemail" lines, kept so their text
/// can be updated.
static MISSED_ITEM: OnceLock<MenuItem<tauri::Wry>> = OnceLock::new();
static VOICEMAIL_ITEM: OnceLock<MenuItem<tauri::Wry>> = OnceLock::new();
static DND_ITEM: OnceLock<CheckMenuItem<tauri::Wry>> = OnceLock::new();

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open SIP Softphone", true, None::<&str>)?;
    let missed = MenuItem::with_id(app, "missed", "No missed calls", false, None::<&str>)?;
    let voicemail = MenuItem::with_id(app, "voicemail", "Call voicemail", true, None::<&str>)?;
    let dnd = CheckMenuItem::with_id(app, "dnd", "Do Not Disturb", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &missed,
            &voicemail,
            &PredefinedMenuItem::separator(app)?,
            &dnd,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let _ = DND_ITEM.set(dnd);
    let _ = MISSED_ITEM.set(missed);
    let _ = VOICEMAIL_ITEM.set(voicemail);

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(TRAY_ICON)?)
        .tooltip("SIP Softphone")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "missed" => {
                show_main(app);
                let _ = app.emit("show-recents", ());
            }
            "voicemail" => {
                show_main(app);
                let _ = app.emit("call-voicemail", ());
            }
            // The UI owns the setting; it calls sip_set_dnd back.
            "dnd" => {
                let _ = app.emit("toggle-dnd", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Called by the UI whenever the missed-call count, voicemail count or
/// registration changes. `voicemail` is the number of new messages; `Some(0)`
/// with `voicemail_waiting` means the PBX says "yes" without a count.
#[tauri::command]
fn set_tray_state(
    app: AppHandle,
    missed: u32,
    status: String,
    voicemail: Option<u32>,
    voicemail_waiting: Option<bool>,
) -> Result<(), String> {
    let tray = app.tray_by_id("main").ok_or("no tray")?;
    let vm = voicemail.unwrap_or(0);
    let vm_waiting = voicemail_waiting.unwrap_or(vm > 0);
    let icon = if missed > 0 || vm_waiting { TRAY_ICON_MISSED } else { TRAY_ICON };
    tray.set_icon(Some(Image::from_bytes(icon).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    let line = match missed {
        0 => "No missed calls".to_string(),
        1 => "1 missed call".to_string(),
        n => format!("{n} missed calls"),
    };
    let vm_line = match (vm, vm_waiting) {
        (0, false) => "No new voicemail".to_string(),
        (0, true) => "New voicemail".to_string(),
        (1, _) => "1 new voicemail".to_string(),
        (n, _) => format!("{n} new voicemails"),
    };
    let _ = tray.set_tooltip(Some(format!("SIP Softphone: {status}. {line}. {vm_line}")));
    if let Some(item) = MISSED_ITEM.get() {
        let _ = item.set_text(&line);
        let _ = item.set_enabled(missed > 0);
    }
    if let Some(item) = VOICEMAIL_ITEM.get() {
        // Always clickable: it dials the voicemail number.
        let _ = item.set_text(if vm_waiting { format!("{vm_line}: listen") } else { "Call voicemail".to_string() });
        let _ = item.set_enabled(true);
    }
    Ok(())
}

#[tauri::command]
fn audio_devices() -> Vec<sipcore::AudioDevice> {
    sipcore::audio_devices()
}

#[tauri::command]
fn set_audio_devices(capture: Option<i32>, playback: Option<i32>) -> Result<(), String> {
    sipcore::set_audio_devices(capture, playback).map_err(err)
}

// ---------- updates ----------

#[derive(serde::Serialize)]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Asks the release feed whether there's a newer version.
#[tauri::command]
async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo { version: u.version, notes: u.body }))
}

/// Downloads the update, checks its signature, replaces the installed app and
/// starts the new version. Emits "update-progress" with the percent done.
#[tauri::command]
async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("You're already on the latest version.")?;
    log_line(&format!("downloading update {}", update.version));
    let mut done = 0usize;
    let progress = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                done += chunk;
                if let Some(total) = total.filter(|t| *t > 0) {
                    let _ = progress.emit("update-progress", (done as u64 * 100 / total).min(100));
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    // Sign out before the files are swapped; on Windows the installer closes
    // the app without the usual exit path.
    sipcore::hangup_all();
    sipcore::shutdown();
    if let Err(e) = update.install(bytes) {
        log_line(&format!("update failed: {e}"));
        // The phone is stopped, so start it again on the version that's there.
        app.restart();
    }
    log_line("update installed, restarting");
    app.restart();
}

/// Opens the latest release's page, for downloading by hand when the in-app
/// update can't install. Derived from the update feed so it follows RELEASE_REPO.
#[tauri::command]
fn open_releases(app: AppHandle) -> Result<(), String> {
    let feed = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u["endpoints"][0].as_str())
        .ok_or("No update feed configured")?;
    let url = feed.trim_end_matches("/download/latest.json");
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        // Must come first: a second launch (e.g. from a tel: link) hands its
        // arguments to this one and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| show_main(app)))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            if std::env::args().any(|a| a == MINIMIZED_ARG) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            if let Ok(dir) = app.path().app_log_dir() {
                open_log(dir);
            }
            log_line(&format!(
                "SIP Softphone {} on {} {}",
                app.package_info().version,
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
            sipcore::start(
                move |ev: SipEvent| {
                    write_log(&ev);
                    if let SipEvent::Incoming { .. } = ev {
                        // Bring the phone forward so the call can be answered.
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.unminimize();
                            let _ = w.show();
                            let _ = w.set_focus();
                            let _ = w.request_user_attention(Some(UserAttentionType::Critical));
                        }
                    }
                    let _ = handle.emit("sip", &ev);
                },
                4,
            )
            .map_err(|e| e.to_string())?;
            build_tray(app.handle())?;
            // Click-to-dial. AppImage and dev builds aren't registered by an
            // installer, so register the link types at runtime there.
            #[cfg(any(windows, target_os = "linux"))]
            if let Err(e) = app.deep_link().register_all() {
                log_line(&format!("link registration failed: {e}"));
            }
            let link_handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                on_dial_links(&link_handle, event.urls().iter().map(|u| u.to_string()).collect());
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                on_dial_links(app.handle(), urls.iter().map(|u| u.to_string()).collect());
            }
            // Record what audio hardware pjsip sees, for troubleshooting.
            for d in sipcore::audio_devices() {
                log_line(&format!(
                    "audio device {}: {} ({}) in={} out={}",
                    d.index, d.name, d.driver, d.inputs, d.outputs
                ));
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the phone running in the tray so calls
            // still ring. Quit from the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            sip_register,
            sip_unregister,
            sip_call,
            sip_answer,
            sip_hangup,
            sip_dtmf,
            sip_mute,
            sip_hold,
            sip_transfer,
            sip_transfer_attended,
            sip_ring,
            ringtones,
            set_tray_state,
            show_log,
            audio_devices,
            set_audio_devices,
            sip_set_dnd,
            sip_set_forward,
            sip_watch,
            password_get,
            password_set,
            password_delete,
            take_dial_link,
            set_hotkeys,
            get_autostart,
            set_autostart,
            app_version,
            update_check,
            update_install,
            open_releases
        ])
        .build(tauri::generate_context!())
        .expect("failed to build app")
        .run(|app, event| match event {
            // Clicking the Dock icon brings the hidden window back.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => show_main(app),
            tauri::RunEvent::Exit => {
                sipcore::hangup_all();
                sipcore::shutdown();
            }
            _ => {
                let _ = app;
            }
        });
}

// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Deserialize;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use sipcore::{Account, SipEvent, Transport};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, UserAttentionType, WindowEvent};

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

// ---------- tray / menu bar icon ----------

/// The tray menu's "missed calls" line, kept so its text can be updated.
static MISSED_ITEM: OnceLock<MenuItem<tauri::Wry>> = OnceLock::new();

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
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &missed, &PredefinedMenuItem::separator(app)?, &quit])?;
    let _ = MISSED_ITEM.set(missed);

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

/// Called by the UI whenever the missed-call count or registration changes.
#[tauri::command]
fn set_tray_state(app: AppHandle, missed: u32, status: String) -> Result<(), String> {
    let tray = app.tray_by_id("main").ok_or("no tray")?;
    let icon = if missed > 0 { TRAY_ICON_MISSED } else { TRAY_ICON };
    tray.set_icon(Some(Image::from_bytes(icon).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    let line = match missed {
        0 => "No missed calls".to_string(),
        1 => "1 missed call".to_string(),
        n => format!("{n} missed calls"),
    };
    let _ = tray.set_tooltip(Some(format!("SIP Softphone: {status}. {line}")));
    if let Some(item) = MISSED_ITEM.get() {
        let _ = item.set_text(&line);
        let _ = item.set_enabled(missed > 0);
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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            if let Ok(dir) = app.path().app_log_dir() {
                open_log(dir);
            }
            log_line(&format!(
                "SIP Softphone {} on {} {}",
                env!("CARGO_PKG_VERSION"),
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
            set_audio_devices
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

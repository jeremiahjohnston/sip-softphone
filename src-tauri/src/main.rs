// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Deserialize;
use sipcore::{Account, SipEvent, Transport};
use tauri::{Emitter, Manager, UserAttentionType};

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
fn sip_ring(mode: sipcore::Ring) -> Result<(), String> {
    sipcore::ring(mode).map_err(err)
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
            sipcore::start(
                move |ev: SipEvent| {
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
            Ok(())
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
            audio_devices,
            set_audio_devices
        ])
        .build(tauri::generate_context!())
        .expect("failed to build app")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                sipcore::hangup_all();
                sipcore::shutdown();
            }
        });
}

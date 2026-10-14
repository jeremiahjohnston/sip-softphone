//! Safe wrapper around the pjsua shim in `csrc/`.
//!
//! There is one SIP stack per process. Call [`start`] once with an event
//! handler, then use the free functions to drive it.

use serde::Serialize;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::OnceLock;

#[repr(C)]
struct RawEvent {
    kind: c_int,
    call_id: c_int,
    code: c_int,
    text: *const c_char,
    remote: *const c_char,
}

type RawCallback = extern "C" fn(*const RawEvent, *mut c_void);

extern "C" {
    fn sp_init(cb: RawCallback, user: *mut c_void, log_level: c_int) -> c_int;
    fn sp_set_account(
        server: *const c_char,
        port: c_int,
        transport: *const c_char,
        user: *const c_char,
        auth_user: *const c_char,
        password: *const c_char,
        display_name: *const c_char,
    ) -> c_int;
    fn sp_remove_account() -> c_int;
    fn sp_make_call(dest: *const c_char) -> c_int;
    fn sp_answer(call_id: c_int) -> c_int;
    fn sp_hangup(call_id: c_int) -> c_int;
    fn sp_hangup_all();
    fn sp_send_dtmf(call_id: c_int, digits: *const c_char) -> c_int;
    fn sp_set_mute(call_id: c_int, mute: c_int) -> c_int;
    fn sp_set_hold(call_id: c_int, hold: c_int) -> c_int;
    fn sp_transfer(call_id: c_int, dest: *const c_char) -> c_int;
    fn sp_transfer_attended(call_id: c_int, dest_call_id: c_int) -> c_int;
    fn sp_audio_dev_count() -> c_int;
    fn sp_audio_dev_info(
        idx: c_int,
        name: *mut c_char,
        name_len: c_int,
        driver: *mut c_char,
        driver_len: c_int,
        inputs: *mut c_int,
        outputs: *mut c_int,
    ) -> c_int;
    fn sp_set_audio_devs(capture: c_int, playback: c_int) -> c_int;
    fn sp_ring(mode: c_int, tone: c_int) -> c_int;
    fn sp_ringtone_count() -> c_int;
    fn sp_use_null_audio() -> c_int;
    fn sp_shutdown();
}

/// Events coming up from the SIP stack. Serialized with a `type` tag so the
/// UI can switch on it directly.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SipEvent {
    Registration { registered: bool, code: i32, reason: String },
    Incoming { call_id: i32, remote: String },
    CallState { call_id: i32, state: String, code: i32, remote: String },
    Media { call_id: i32, active: bool, status: String },
    Log { level: i32, line: String },
    /// Progress of a transfer we started on `call_id`.
    Transfer { call_id: i32, code: i32, reason: String, done: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Udp,
    Tcp,
    Tls,
}

impl Transport {
    fn as_cstr(self) -> &'static CStr {
        match self {
            Transport::Udp => c"udp",
            Transport::Tcp => c"tcp",
            Transport::Tls => c"tls",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub server: String,
    /// 0 uses the transport's default port.
    pub port: u16,
    pub transport: Transport,
    pub user: String,
    /// Leave empty to authenticate with `user`.
    pub auth_user: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub index: i32,
    pub name: String,
    pub driver: String,
    pub inputs: u32,
    pub outputs: u32,
}

#[derive(Debug)]
pub struct Error(pub i32, pub &'static str);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} failed (pj status {})", self.1, self.0)
    }
}

impl std::error::Error for Error {}

type Handler = Box<dyn Fn(SipEvent) + Send + Sync>;
static HANDLER: OnceLock<Handler> = OnceLock::new();

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

extern "C" fn trampoline(ev: *const RawEvent, _user: *mut c_void) {
    let Some(handler) = HANDLER.get() else { return };
    let ev = unsafe { &*ev };
    let event = match ev.kind {
        1 => SipEvent::Registration {
            registered: ev.call_id == 1,
            code: ev.code,
            reason: cstr(ev.text),
        },
        2 => SipEvent::Incoming { call_id: ev.call_id, remote: cstr(ev.remote) },
        3 => SipEvent::CallState {
            call_id: ev.call_id,
            state: cstr(ev.text),
            code: ev.code,
            remote: cstr(ev.remote),
        },
        4 => SipEvent::Media { call_id: ev.call_id, active: ev.code == 1, status: cstr(ev.text) },
        5 => SipEvent::Log { level: ev.code, line: cstr(ev.text) },
        6 => SipEvent::Transfer {
            call_id: ev.call_id,
            code: ev.code,
            reason: cstr(ev.text),
            done: cstr(ev.remote) == "final",
        },
        _ => return,
    };
    // Never let a panic unwind into C.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(event)));
}

fn check(status: c_int, what: &'static str) -> Result<(), Error> {
    if status == 0 {
        Ok(())
    } else {
        Err(Error(status, what))
    }
}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}

/// Starts the SIP stack. The handler is called from SIP worker threads.
/// Only the first call's handler is kept.
pub fn start<F>(handler: F, log_level: i32) -> Result<(), Error>
where
    F: Fn(SipEvent) + Send + Sync + 'static,
{
    let _ = HANDLER.set(Box::new(handler));
    check(unsafe { sp_init(trampoline, std::ptr::null_mut(), log_level) }, "start")
}

/// Registers the account, replacing any previous one. The result arrives
/// later as a [`SipEvent::Registration`].
pub fn set_account(acc: &Account) -> Result<(), Error> {
    let (server, user, auth, pass, name) = (
        c(&acc.server),
        c(&acc.user),
        c(&acc.auth_user),
        c(&acc.password),
        c(&acc.display_name),
    );
    check(
        unsafe {
            sp_set_account(
                server.as_ptr(),
                acc.port as c_int,
                acc.transport.as_cstr().as_ptr(),
                user.as_ptr(),
                auth.as_ptr(),
                pass.as_ptr(),
                name.as_ptr(),
            )
        },
        "register",
    )
}

pub fn remove_account() -> Result<(), Error> {
    check(unsafe { sp_remove_account() }, "unregister")
}

/// Dials an extension, number or full `sip:` URI. Returns the call id.
pub fn call(dest: &str) -> Result<i32, Error> {
    let d = c(dest.trim());
    match unsafe { sp_make_call(d.as_ptr()) } {
        id if id >= 0 => Ok(id),
        _ => Err(Error(-1, "call")),
    }
}

pub fn answer(call_id: i32) -> Result<(), Error> {
    check(unsafe { sp_answer(call_id) }, "answer")
}

pub fn hangup(call_id: i32) -> Result<(), Error> {
    check(unsafe { sp_hangup(call_id) }, "hangup")
}

pub fn hangup_all() {
    unsafe { sp_hangup_all() }
}

pub fn send_dtmf(call_id: i32, digits: &str) -> Result<(), Error> {
    let d = c(digits);
    check(unsafe { sp_send_dtmf(call_id, d.as_ptr()) }, "dtmf")
}

pub fn set_mute(call_id: i32, mute: bool) -> Result<(), Error> {
    check(unsafe { sp_set_mute(call_id, mute as c_int) }, "mute")
}

pub fn set_hold(call_id: i32, hold: bool) -> Result<(), Error> {
    check(unsafe { sp_set_hold(call_id, hold as c_int) }, "hold")
}

/// Blind transfer: sends the other party on `call_id` to `dest`.
pub fn transfer(call_id: i32, dest: &str) -> Result<(), Error> {
    let d = c(dest.trim());
    check(unsafe { sp_transfer(call_id, d.as_ptr()) }, "transfer")
}

/// Attended transfer: connects the party on `call_id` to the party on
/// `dest_call_id` (usually a consult call you placed while `call_id` was held).
pub fn transfer_attended(call_id: i32, dest_call_id: i32) -> Result<(), Error> {
    check(unsafe { sp_transfer_attended(call_id, dest_call_id) }, "transfer")
}

/// Lists sound devices. Plugged-in devices show up when no call is active.
pub fn audio_devices() -> Vec<AudioDevice> {
    let count = unsafe { sp_audio_dev_count() };
    (0..count)
        .filter_map(|i| {
            let mut name = [0 as c_char; 256];
            let mut driver = [0 as c_char; 64];
            let (mut inputs, mut outputs) = (0, 0);
            let status = unsafe {
                sp_audio_dev_info(
                    i,
                    name.as_mut_ptr(),
                    name.len() as c_int,
                    driver.as_mut_ptr(),
                    driver.len() as c_int,
                    &mut inputs,
                    &mut outputs,
                )
            };
            (status == 0).then(|| AudioDevice {
                index: i,
                name: cstr(name.as_ptr()),
                driver: cstr(driver.as_ptr()),
                inputs: inputs.max(0) as u32,
                outputs: outputs.max(0) as u32,
            })
        })
        .collect()
}

/// Picks the microphone and speaker by [`AudioDevice::index`]. `None` uses
/// the system default. Takes effect immediately, including during a call.
pub fn set_audio_devices(capture: Option<i32>, playback: Option<i32>) -> Result<(), Error> {
    check(
        unsafe { sp_set_audio_devs(capture.unwrap_or(-1), playback.unwrap_or(-1)) },
        "audio device",
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ring {
    Off,
    /// Looping ringtone for an incoming call.
    Ring,
    /// A short double beep for a second call arriving mid-call.
    Waiting,
    /// Plays the ringtone once, for trying it out in settings.
    Preview,
}

/// Names of the built-in ringtones, in the order `ring` expects.
pub const RINGTONES: &[&str] = &["Chirp", "Classic", "British", "Digital", "Chime", "Soft", "Office"];

/// Plays or stops the ringer on the selected speaker. `tone` indexes [`RINGTONES`].
pub fn ring(mode: Ring, tone: usize) -> Result<(), Error> {
    let m = match mode {
        Ring::Off => 0,
        Ring::Ring => 1,
        Ring::Waiting => 2,
        Ring::Preview => 3,
    };
    check(unsafe { sp_ring(m, tone as c_int) }, "ring")
}

/// Number of ringtones the C side knows; should equal `RINGTONES.len()`.
pub fn ringtone_count() -> usize {
    unsafe { sp_ringtone_count() as usize }
}

/// Runs without a sound device. Meant for headless tests.
pub fn use_null_audio() -> Result<(), Error> {
    check(unsafe { sp_use_null_audio() }, "null audio")
}

pub fn shutdown() {
    unsafe { sp_shutdown() }
}

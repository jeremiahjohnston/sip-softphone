//! Command-line check of the SIP core without the UI.
//!
//!   SIP_SERVER=pbx.local SIP_USER=1001 SIP_PASS=... [SIP_TRANSPORT=udp] \
//!   [SIP_DIAL=1002] cargo run --example smoke
//!
//! SIP_XFER=<dest> transfers the call there (SIP_ATTENDED=1 consults first).
//! SIP_WAIT=<secs> stays up for incoming calls, SIP_ANSWER=1 answers them.
//! SIP_NULL_AUDIO=1 runs without a sound device, SIP_LOG=1 prints pjsip logs.
//! With no SIP_SERVER it just starts and stops the stack.

use sipcore::{Account, SipEvent, Transport};
use std::{env, thread, time::Duration};

fn main() {
    sipcore::start(
        |ev| match ev {
            SipEvent::Log { line, .. } => {
                if env::var_os("SIP_LOG").is_some() {
                    println!("  log: {line}");
                }
            }
            SipEvent::Incoming { call_id, ref remote } if env::var_os("SIP_ANSWER").is_some() => {
                println!("incoming call {call_id} from {remote}");
                println!("answer: {:?}", sipcore::answer(call_id));
            }
            other => println!("{other:?}"),
        },
        4,
    )
    .expect("start");
    println!("SIP stack started");
    assert_eq!(sipcore::ringtone_count(), sipcore::RINGTONES.len(), "ringtone tables out of sync");
    if env::var_os("SIP_RING").is_some() {
        println!("ring: {:?}", sipcore::ring(sipcore::Ring::Ring, 4));
        thread::sleep(Duration::from_secs(3));
        println!("waiting: {:?}", sipcore::ring(sipcore::Ring::Waiting, 0));
        thread::sleep(Duration::from_secs(1));
        println!("off: {:?}", sipcore::ring(sipcore::Ring::Off, 0));
    }
    for dev in sipcore::audio_devices() {
        println!("audio device {dev:?}");
    }
    if env::var_os("SIP_NULL_AUDIO").is_some() {
        sipcore::use_null_audio().expect("null audio");
    }

    if let Ok(server) = env::var("SIP_SERVER") {
        let transport = match env::var("SIP_TRANSPORT").as_deref() {
            Ok("tcp") => Transport::Tcp,
            Ok("tls") => Transport::Tls,
            _ => Transport::Udp,
        };
        sipcore::set_account(&Account {
            server,
            port: env::var("SIP_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(0),
            transport,
            user: env::var("SIP_USER").unwrap_or_default(),
            auth_user: String::new(),
            password: env::var("SIP_PASS").unwrap_or_default(),
            display_name: "Smoke test".into(),
        })
        .expect("register");
        thread::sleep(Duration::from_secs(3));

        if let Ok(dest) = env::var("SIP_DIAL") {
            let id = sipcore::call(&dest).expect("call");
            println!("dialing {dest} as call {id}");
            thread::sleep(Duration::from_secs(6));
            if let Ok(xfer) = env::var("SIP_XFER") {
                if env::var_os("SIP_ATTENDED").is_some() {
                    println!("hold A: {:?}", sipcore::set_hold(id, true));
                    let b = sipcore::call(&xfer).expect("consult call");
                    thread::sleep(Duration::from_secs(4));
                    println!("attended transfer: {:?}", sipcore::transfer_attended(id, b));
                } else {
                    println!("blind transfer: {:?}", sipcore::transfer(id, &xfer));
                }
                thread::sleep(Duration::from_secs(6));
                sipcore::hangup_all();
                thread::sleep(Duration::from_secs(1));
                sipcore::shutdown();
                return;
            }
            println!("dtmf: {:?}", sipcore::send_dtmf(id, "123"));
            println!("mute: {:?}", sipcore::set_mute(id, true));
            println!("hold: {:?}", sipcore::set_hold(id, true));
            thread::sleep(Duration::from_secs(2));
            println!("unhold: {:?}", sipcore::set_hold(id, false));
            thread::sleep(Duration::from_secs(2));
            println!("hangup: {:?}", sipcore::hangup(id));
            thread::sleep(Duration::from_secs(2));
        }
    }

    if let Some(secs) = env::var("SIP_WAIT").ok().and_then(|s| s.parse().ok()) {
        println!("waiting {secs}s for incoming calls");
        thread::sleep(Duration::from_secs(secs));
    }

    sipcore::shutdown();
    println!("SIP stack stopped");
}

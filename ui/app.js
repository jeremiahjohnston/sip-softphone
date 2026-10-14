// UI for the softphone. Talks to the Rust side through Tauri commands and
// listens for "sip" events (see sipcore::SipEvent for the shapes).

const tauri = window.__TAURI__;
const invoke = tauri ? tauri.core.invoke : mockInvoke;
const listen = tauri ? tauri.event.listen : mockListen;

const $ = (id) => document.getElementById(id);
const KEYS = [
  ["1", ""], ["2", "ABC"], ["3", "DEF"],
  ["4", "GHI"], ["5", "JKL"], ["6", "MNO"],
  ["7", "PQRS"], ["8", "TUV"], ["9", "WXYZ"],
  ["*", ""], ["0", "+"], ["#", ""],
];
const STORE_KEY = "softphone.account";
const AUDIO_KEY = "softphone.audio";
const STATE_TEXT = {
  calling: "Calling…",
  ringing: "Ringing…",
  incoming: "Incoming call",
  connecting: "Connecting…",
  confirmed: "Connected",
  disconnected: "Call ended",
};

let call = null; // { id, remote, state, incoming, muted, held, since }
let timer = null;

// ---------- views ----------

function show(view) {
  for (const v of ["dial", "call", "settings"]) {
    $("view-" + v).classList.toggle("hidden", v !== view);
  }
}

function toast(msg) {
  const t = $("toast");
  t.textContent = msg;
  t.classList.remove("hidden");
  clearTimeout(toast.t);
  toast.t = setTimeout(() => t.classList.add("hidden"), 3500);
}

function buildPad(el, onKey) {
  for (const [d, letters] of KEYS) {
    const b = document.createElement("button");
    b.innerHTML = `${d}<small>${letters || "&nbsp;"}</small>`;
    b.addEventListener("click", () => onKey(d));
    el.appendChild(b);
  }
}

function prettyRemote(uri) {
  // '"Jane" <sip:1002@pbx>' -> 'Jane', '<sip:1002@pbx>' -> '1002'
  const name = uri.match(/^\s*"([^"]+)"/);
  if (name) return name[1];
  const user = uri.match(/sips?:([^@>;]+)@/);
  return user ? user[1] : uri.replace(/[<>]/g, "");
}

function renderCall() {
  if (!call) {
    clearInterval(timer);
    timer = null;
    show("dial");
    return;
  }
  show("call");
  $("call-remote").textContent = prettyRemote(call.remote);
  let state = STATE_TEXT[call.state] || call.state;
  if (call.held) state = "On hold";
  $("call-state").textContent = state;
  const ringingIn = call.incoming && call.state !== "confirmed" && call.state !== "connecting";
  $("incoming-actions").classList.toggle("hidden", !ringingIn || call.state === "disconnected");
  $("call-actions").classList.toggle("hidden", ringingIn);
  $("btn-mute").classList.toggle("on", call.muted);
  $("btn-hold").classList.toggle("on", call.held);
  const live = call.state === "confirmed";
  $("btn-mute").disabled = $("btn-hold").disabled = $("btn-keypad").disabled = !live;
  if (live && !timer) {
    call.since = Date.now();
    timer = setInterval(tick, 1000);
    tick();
  }
  if (!live) $("call-timer").textContent = "";
}

function tick() {
  if (!call || !call.since) return;
  const s = Math.floor((Date.now() - call.since) / 1000);
  $("call-timer").textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// ---------- registration ----------

function setReg(kind, text) {
  $("reg-dot").className = "dot " + kind;
  $("reg-text").textContent = text;
}

function loadAccount() {
  try {
    return JSON.parse(localStorage.getItem(STORE_KEY)) || {};
  } catch {
    return {};
  }
}

function fillForm() {
  const acc = loadAccount();
  const f = $("acct-form");
  for (const k of ["server", "port", "transport", "user", "auth_user", "display_name"]) {
    if (acc[k] != null) f.elements[k].value = acc[k];
  }
  if (acc.user && acc.server) $("acct-name").textContent = `${acc.user}@${acc.server}`;
}

async function register(e) {
  e.preventDefault();
  const f = $("acct-form");
  const data = Object.fromEntries(new FormData(f));
  const account = {
    server: data.server.trim(),
    port: data.port ? Number(data.port) : null,
    transport: data.transport,
    user: data.user.trim(),
    auth_user: data.auth_user.trim() || null,
    password: data.password,
    display_name: data.display_name.trim() || null,
  };
  const { password, ...saved } = account;
  try { localStorage.setItem(STORE_KEY, JSON.stringify(saved)); } catch {}
  $("acct-name").textContent = `${account.user}@${account.server}`;
  setReg("pending", "Registering…");
  try {
    await invoke("sip_register", { account });
    show(call ? "call" : "dial");
  } catch (err) {
    setReg("err", String(err));
  }
}

// ---------- audio devices ----------

// Saved by device name, since device numbers can change between launches.
function loadAudio() {
  try {
    return JSON.parse(localStorage.getItem(AUDIO_KEY)) || {};
  } catch {
    return {};
  }
}

function fillSelect(sel, devices, saved) {
  sel.innerHTML = "";
  const def = new Option("System default", "");
  sel.add(def);
  for (const d of devices) sel.add(new Option(d.name, d.name));
  sel.value = devices.some((d) => d.name === saved) ? saved : "";
}

async function loadDevices() {
  const devices = await invoke("audio_devices");
  const saved = loadAudio();
  fillSelect($("mic"), devices.filter((d) => d.inputs > 0), saved.mic);
  fillSelect($("speaker"), devices.filter((d) => d.outputs > 0), saved.speaker);
  return devices;
}

async function applyAudio(force) {
  const devices = await loadDevices();
  const mic = $("mic").value;
  const speaker = $("speaker").value;
  // Nothing chosen: leave pjsip on its defaults (and don't open the mic at launch).
  if (!force && !mic && !speaker) return;
  const find = (name, key) => devices.find((d) => d.name === name && d[key] > 0);
  try {
    await invoke("set_audio_devices", {
      capture: mic ? find(mic, "inputs")?.index ?? null : null,
      playback: speaker ? find(speaker, "outputs")?.index ?? null : null,
    });
  } catch (err) {
    toast("Couldn't switch audio device.");
  }
}

async function onAudioChange() {
  try {
    localStorage.setItem(AUDIO_KEY, JSON.stringify({ mic: $("mic").value, speaker: $("speaker").value }));
  } catch {}
  await applyAudio(true);
}

// ---------- calls ----------

async function dial() {
  const dest = $("dial").value.trim();
  if (!dest || call) return;
  try {
    const id = await invoke("sip_call", { dest });
    call = { id, remote: dest, state: "calling", incoming: false, muted: false, held: false };
    renderCall();
  } catch (err) {
    toast("Couldn't place the call. Check that you're registered.");
  }
}

function onSip(ev) {
  switch (ev.type) {
    case "registration":
      if (ev.registered) setReg("ok", "Registered");
      else if (ev.code >= 300 || ev.code === 0) setReg("err", `${ev.code || ""} ${ev.reason || "Not registered"}`.trim());
      else setReg("", "Not registered");
      break;
    case "incoming":
      if (call && call.id !== ev.call_id) {
        invoke("sip_hangup", { callId: ev.call_id }); // busy: one call at a time for now
        return;
      }
      call = { id: ev.call_id, remote: ev.remote, state: "incoming", incoming: true, muted: false, held: false };
      renderCall();
      break;
    case "call_state":
      if (!call || call.id !== ev.call_id) return;
      call.state = call.incoming && ev.state === "ringing" ? "incoming" : ev.state;
      if (ev.remote) call.remote = ev.remote;
      renderCall();
      if (ev.state === "disconnected") {
        if (ev.code >= 400) toast(`Call ended (${ev.code})`);
        const ended = call;
        setTimeout(() => {
          if (call === ended) {
            call = null;
            renderCall();
          }
        }, 1200);
      }
      break;
    case "media":
      if (call && call.id === ev.call_id) {
        call.held = ev.status === "local_hold";
        renderCall();
      }
      break;
    case "log": {
      const log = $("log");
      log.textContent += ev.line + "\n";
      if (log.textContent.length > 60000) log.textContent = log.textContent.slice(-40000);
      break;
    }
  }
}

// ---------- wiring ----------

buildPad($("pad"), (d) => { $("dial").value += d; $("dial").focus(); });
buildPad($("dtmf-pad"), (d) => call && invoke("sip_dtmf", { callId: call.id, digits: d }));

$("btn-call").addEventListener("click", dial);
$("dial").addEventListener("keydown", (e) => e.key === "Enter" && dial());
$("btn-answer").addEventListener("click", () => call && invoke("sip_answer", { callId: call.id }));
$("btn-decline").addEventListener("click", () => call && invoke("sip_hangup", { callId: call.id }));
$("btn-hangup").addEventListener("click", () => call && invoke("sip_hangup", { callId: call.id }));
$("btn-mute").addEventListener("click", async () => {
  if (!call) return;
  await invoke("sip_mute", { callId: call.id, mute: !call.muted });
  call.muted = !call.muted;
  renderCall();
});
$("btn-hold").addEventListener("click", () => call && invoke("sip_hold", { callId: call.id, hold: !call.held }));
$("btn-keypad").addEventListener("click", () => {
  $("dtmf-pad").classList.toggle("hidden");
  $("btn-keypad").classList.toggle("on");
});
$("btn-settings").addEventListener("click", () => {
  const open = !$("view-settings").classList.contains("hidden");
  if (open) renderCall();
  else {
    show("settings");
    loadDevices();
  }
});
$("mic").addEventListener("change", onAudioChange);
$("speaker").addEventListener("change", onAudioChange);
$("acct-form").addEventListener("submit", register);
$("btn-unregister").addEventListener("click", async () => {
  try { await invoke("sip_unregister"); } catch {}
  setReg("", "Not registered");
});

fillForm();
listen("sip", (e) => onSip(e.payload));
applyAudio();
if (!loadAccount().server) show("settings");

// ---------- browser preview (no Tauri) ----------

function mockInvoke(cmd, args) {
  console.log("invoke", cmd, args);
  if (cmd === "sip_register") setTimeout(() => onSip({ type: "registration", registered: true, code: 200, reason: "OK" }), 400);
  if (cmd === "sip_call") return Promise.resolve(0);
  if (cmd === "audio_devices")
    return Promise.resolve([
      { index: 0, name: "MacBook Pro Microphone", driver: "core audio", inputs: 1, outputs: 0 },
      { index: 1, name: "MacBook Pro Speakers", driver: "core audio", inputs: 0, outputs: 2 },
      { index: 2, name: "Jabra Evolve2 65", driver: "core audio", inputs: 1, outputs: 2 },
    ]);
  return Promise.resolve();
}
function mockListen() {
  return Promise.resolve(() => {});
}

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
const STORE = {
  account: "softphone.account",
  audio: "softphone.audio",
  recents: "softphone.recents",
  contacts: "softphone.contacts",
  missed: "softphone.missed",
};
const STATE_TEXT = {
  calling: "Calling…",
  ringing: "Ringing…",
  incoming: "Incoming call",
  connecting: "Connecting…",
  confirmed: "Connected",
  disconnected: "Call ended",
};
const MAX_RECENTS = 200;

// id -> { id, remote, number, state, incoming, answered, muted, held, since, startedAt, ended }
const calls = new Map();
let focusId = null;
let tab = "dial";
let settingsOpen = false;
let ticker = null;

// ---------- storage ----------

function load(key, fallback) {
  try {
    const v = JSON.parse(localStorage.getItem(key));
    return v ?? fallback;
  } catch {
    return fallback;
  }
}

function save(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {}
}

// ---------- helpers ----------

function toast(msg) {
  const t = $("toast");
  t.textContent = msg;
  t.classList.remove("hidden");
  clearTimeout(toast.t);
  toast.t = setTimeout(() => t.classList.add("hidden"), 3500);
}

function el(tag, props = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v);
  }
  for (const k of kids) if (k != null) e.append(k);
  return e;
}

function icon(name) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  const use = document.createElementNS(ns, "use");
  use.setAttribute("href", "#i-" + name);
  svg.append(use);
  return svg;
}

function buildPad(target, onKey) {
  for (const [d, letters] of KEYS) {
    const b = el("button", { onclick: () => onKey(d) }, d, el("small", {}, letters || " "));
    target.append(b);
  }
}

// '"Jane" <sip:1002@pbx>' -> { name: "Jane", number: "1002" }
function parseRemote(uri) {
  const name = (uri.match(/^\s*"([^"]+)"/) || [])[1] || "";
  const user = (uri.match(/sips?:([^@>;]+)@/) || [])[1];
  const number = user || uri.replace(/[<>]/g, "").replace(/^sips?:/, "");
  return { name, number };
}

const normalize = (n) => String(n || "").replace(/[\s\-().]/g, "").toLowerCase();

function contactFor(number) {
  const n = normalize(number);
  return n ? load(STORE.contacts, []).find((c) => normalize(c.number) === n) : undefined;
}

function displayName(number, fallbackName) {
  return contactFor(number)?.name || fallbackName || number;
}

function initials(name) {
  const parts = String(name).trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return "#";
  if (/^\d/.test(parts[0])) return "#";
  return (parts[0][0] + (parts[1]?.[0] || "")).toUpperCase();
}

function fmtDuration(s) {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

function fmtWhen(ts) {
  const d = new Date(ts);
  const now = new Date();
  const time = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (d.toDateString() === now.toDateString()) return time;
  const y = new Date(now);
  y.setDate(now.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return "Yesterday";
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

// ---------- layout ----------

function render() {
  const inCall = calls.size > 0;
  const view = settingsOpen ? "settings" : inCall ? "call" : tab;
  for (const v of ["dial", "recents", "contacts", "call", "settings"]) {
    $("view-" + v).classList.toggle("hidden", v !== view);
  }
  $("tabs").classList.toggle("hidden", inCall && !settingsOpen);
  for (const b of $("tabs").querySelectorAll("button")) {
    b.classList.toggle("on", !settingsOpen && b.dataset.tab === tab);
  }
  const missed = load(STORE.missed, 0);
  $("missed-badge").textContent = missed;
  $("missed-badge").classList.toggle("hidden", !missed);
  syncTray(missed);
  if (view === "call") renderCalls();
  if (view === "recents") renderRecents();
  if (view === "contacts") renderContacts();
}

// Mirror the missed-call count and registration status on the tray icon.
function syncTray(missed) {
  const status = $("reg-text").textContent;
  const key = missed + "|" + status;
  if (syncTray.last === key) return;
  syncTray.last = key;
  invoke("set_tray_state", { missed, status }).catch(() => {});
}

function setTab(t) {
  tab = t;
  settingsOpen = false;
  if (t === "recents") save(STORE.missed, 0);
  render();
}

// ---------- calls ----------

function addCall(id, remote, incoming) {
  const { name, number } = parseRemote(remote);
  const call = {
    id, remote, name, number, incoming,
    state: incoming ? "incoming" : "calling",
    answered: false, muted: false, held: false, since: 0, startedAt: Date.now(), ended: false,
  };
  calls.set(id, call);
  return call;
}

function liveCalls() {
  return [...calls.values()].filter((c) => !c.ended);
}

function isRingingIn(c) {
  return c.incoming && !c.answered && !c.ended && c.state !== "connecting" && c.state !== "confirmed";
}

function pickFocus() {
  const cur = calls.get(focusId);
  if (cur && (!cur.ended || liveCalls().length === 0)) return;
  const live = liveCalls();
  const next =
    live.find((c) => !c.held && !isRingingIn(c)) ||
    live.find((c) => isRingingIn(c)) ||
    live[0] ||
    [...calls.values()][0];
  focusId = next ? next.id : null;
}

function updateRinger() {
  const ringing = liveCalls().filter(isRingingIn);
  const wantRing = $("ring-on").checked;
  if (ringing.length && wantRing) {
    const busy = liveCalls().some((c) => !isRingingIn(c));
    // One loud ring when idle, a soft beep when it's a second call.
    const mode = busy ? "waiting" : "ring";
    if (updateRinger.mode !== mode) {
      updateRinger.mode = mode;
      invoke("sip_ring", { mode, tone: Number($("ringtone").value) || 0 }).catch(() => {});
    }
  } else if (updateRinger.mode && updateRinger.mode !== "off") {
    updateRinger.mode = "off";
    invoke("sip_ring", { mode: "off" }).catch(() => {});
  }
}

function renderCalls() {
  pickFocus();
  const call = calls.get(focusId);
  if (!call) return;

  const shown = displayName(call.number, call.name);
  $("call-avatar").textContent = initials(shown);
  $("call-remote").textContent = shown;
  $("call-number").textContent = shown !== call.number ? call.number : "";
  let state = STATE_TEXT[call.state] || call.state;
  if (call.held && !call.ended) state = "On hold";
  $("call-state").textContent = state;

  const ringingIn = isRingingIn(call);
  $("incoming-actions").classList.toggle("hidden", !ringingIn);
  $("call-actions").classList.toggle("hidden", ringingIn || call.ended);
  $("btn-mute").classList.toggle("on", call.muted);
  $("btn-mute").querySelector("use").setAttribute("href", call.muted ? "#i-micoff" : "#i-mic");
  $("btn-hold").classList.toggle("on", call.held);
  const live = call.state === "confirmed" && !call.ended;
  for (const id of ["btn-mute", "btn-hold", "btn-keypad", "btn-xfer"]) $(id).disabled = !live;
  if (!live) {
    $("dtmf-pad").classList.add("hidden");
    $("xfer-panel").classList.add("hidden");
    $("btn-keypad").classList.remove("on");
    $("btn-xfer").classList.remove("on");
  }
  tick();

  // Other calls: waiting, held, or being consulted.
  const box = $("other-calls");
  box.innerHTML = "";
  for (const c of liveCalls()) {
    if (c.id === focusId) continue;
    const name = displayName(c.number, c.name);
    const status = isRingingIn(c) ? "Incoming call" : c.held ? "On hold" : STATE_TEXT[c.state] || c.state;
    const buttons = [];
    if (isRingingIn(c)) {
      buttons.push(el("button", { class: "go", onclick: () => answer(c.id) }, "Answer"));
      buttons.push(el("button", { class: "stop", onclick: () => hangup(c.id) }, "Decline"));
    } else {
      if (live && c.state === "confirmed") {
        buttons.push(
          el("button", { class: "alt", title: "Transfer: connect these two calls and leave", onclick: () => connectCalls(c.id, focusId) }, "Connect")
        );
      }
      buttons.push(el("button", { class: "go", onclick: () => resume(c.id) }, "Resume"));
      buttons.push(el("button", { class: "stop", onclick: () => hangup(c.id) }, "End"));
    }
    box.append(
      el("div", { class: "other" },
        el("div", { class: "avatar" }, initials(name)),
        el("div", { class: "main" }, el("div", { class: "name" }, name), el("div", { class: "sub" }, status)),
        ...buttons)
    );
  }
}

function tick() {
  const call = calls.get(focusId);
  if (!call || !call.since || call.ended) {
    $("call-timer").textContent = "";
    return;
  }
  $("call-timer").textContent = fmtDuration(Math.floor((Date.now() - call.since) / 1000));
}

async function dial(dest) {
  dest = String(dest ?? $("dial").value).trim();
  if (!dest) return;
  if (liveCalls().length) {
    toast("Finish the current call first, or use Transfer → Ask first.");
    return;
  }
  try {
    const id = await invoke("sip_call", { dest });
    addCall(id, dest, false);
    focusId = id;
    settingsOpen = false;
    $("dial").value = "";
    render();
  } catch {
    toast("Couldn't place the call. Check that you're registered.");
  }
}

// Put whatever's talking on hold before switching to another call.
async function holdFocused() {
  const cur = calls.get(focusId);
  if (cur && !cur.ended && !cur.held && cur.state === "confirmed") {
    await invoke("sip_hold", { callId: cur.id, hold: true }).catch(() => {});
    cur.held = true;
  }
}

async function answer(id) {
  if (id !== focusId) await holdFocused();
  focusId = id;
  try {
    await invoke("sip_answer", { callId: id });
  } catch {
    toast("Couldn't answer the call.");
  }
  render();
}

async function resume(id) {
  await holdFocused();
  focusId = id;
  const c = calls.get(id);
  if (c?.held) await invoke("sip_hold", { callId: id, hold: false }).catch(() => {});
  render();
}

function hangup(id) {
  invoke("sip_hangup", { callId: id }).catch(() => {});
}

async function connectCalls(heldId, activeId) {
  try {
    await invoke("sip_transfer_attended", { callId: heldId, destCallId: activeId });
    toast("Connecting them…");
  } catch {
    toast("Transfer failed.");
  }
}

async function blindTransfer(e) {
  e.preventDefault();
  const dest = $("xfer-dest").value.trim();
  if (!dest || focusId == null) return;
  try {
    await invoke("sip_transfer", { callId: focusId, dest });
    toast(`Transferring to ${displayName(dest)}…`);
    $("xfer-dest").value = "";
  } catch {
    toast("Transfer failed.");
  }
}

async function consultTransfer() {
  const dest = $("xfer-dest").value.trim();
  if (!dest) return $("xfer-dest").focus();
  await holdFocused();
  try {
    const id = await invoke("sip_call", { dest });
    addCall(id, dest, false);
    focusId = id;
    $("xfer-dest").value = "";
    toast("When they're ready, tap Connect to hand the call over.");
  } catch {
    toast("Couldn't place the call.");
  }
  render();
}

function endCall(call, code) {
  if (call.ended) return;
  call.ended = true;
  addRecent({
    number: call.number,
    name: call.name,
    dir: call.incoming ? (call.answered ? "in" : "missed") : "out",
    at: call.startedAt,
    dur: call.since ? Math.round((Date.now() - call.since) / 1000) : 0,
  });
  if (call.incoming && !call.answered) save(STORE.missed, load(STORE.missed, 0) + 1);
  if (code >= 400 && !call.incoming && code !== 487) toast(`Call ended (${code})`);
  setTimeout(() => {
    calls.delete(call.id);
    if (focusId === call.id) focusId = null;
    render();
  }, 1200);
}

// ---------- recents ----------

function addRecent(r) {
  const list = load(STORE.recents, []);
  list.unshift(r);
  save(STORE.recents, list.slice(0, MAX_RECENTS));
}

function renderRecents() {
  const list = load(STORE.recents, []);
  const ul = $("recents");
  ul.innerHTML = "";
  $("recents-empty").classList.toggle("hidden", list.length > 0);
  for (const r of list) {
    const name = displayName(r.number, r.name);
    const dir = r.dir === "out" ? "out" : "in";
    const detail = r.dir === "missed" ? "Missed" : r.dur ? fmtDuration(r.dur) : r.dir === "out" ? "No answer" : "";
    ul.append(
      el("li", { class: r.dir === "missed" ? "missed" : "", onclick: () => dial(r.number), title: `Call ${r.number}` },
        el("span", { class: "dir" }, icon(dir)),
        el("div", { class: "main" },
          el("div", { class: "name" }, name),
          el("div", { class: "sub" }, [name !== r.number ? r.number : "", detail].filter(Boolean).join(" · "))),
        el("div", { class: "when" }, fmtWhen(r.at)))
    );
  }
}

// ---------- contacts ----------

function contactsSorted() {
  return load(STORE.contacts, []).sort((a, b) => a.name.localeCompare(b.name));
}

function refreshContactList() {
  const dl = $("contact-list");
  dl.innerHTML = "";
  for (const c of contactsSorted()) dl.append(el("option", { value: c.number }, c.name));
}

function renderContacts() {
  const q = $("contact-search").value.trim().toLowerCase();
  const all = contactsSorted();
  const list = all.filter((c) => !q || c.name.toLowerCase().includes(q) || normalize(c.number).includes(normalize(q)));
  const ul = $("contacts");
  ul.innerHTML = "";
  $("contacts-empty").classList.toggle("hidden", all.length > 0);
  for (const c of list) {
    ul.append(
      el("li", { onclick: () => dial(c.number), title: `Call ${c.number}` },
        el("div", { class: "avatar" }, initials(c.name)),
        el("div", { class: "main" }, el("div", { class: "name" }, c.name), el("div", { class: "sub" }, c.number)),
        el("div", { class: "actions" },
          el("button", { class: "call", title: "Call", onclick: (e) => { e.stopPropagation(); dial(c.number); } }, icon("phone")),
          el("button", { title: "Delete", onclick: (e) => { e.stopPropagation(); deleteContact(c.id); } }, icon("trash"))))
    );
  }
}

function deleteContact(id) {
  save(STORE.contacts, load(STORE.contacts, []).filter((c) => c.id !== id));
  refreshContactList();
  render();
}

function saveContact(e) {
  e.preventDefault();
  const f = $("contact-form");
  const name = f.elements.name.value.trim();
  const number = f.elements.number.value.trim();
  if (!name || !number) return;
  const list = load(STORE.contacts, []);
  list.push({ id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6), name, number });
  save(STORE.contacts, list);
  f.reset();
  f.classList.add("hidden");
  refreshContactList();
  render();
}

// ---------- registration ----------

function setReg(kind, text) {
  $("reg-dot").className = "dot " + kind;
  $("reg-text").textContent = text;
  syncTray(load(STORE.missed, 0));
}

function fillForm() {
  const acc = load(STORE.account, {});
  const f = $("acct-form");
  for (const k of ["server", "port", "transport", "user", "auth_user", "display_name"]) {
    if (acc[k] != null) f.elements[k].value = acc[k];
  }
  if (acc.user && acc.server) $("acct-name").textContent = `${acc.user}@${acc.server}`;
}

async function register(e) {
  e.preventDefault();
  const data = Object.fromEntries(new FormData($("acct-form")));
  const account = {
    server: data.server.trim(),
    port: data.port ? Number(data.port) : null,
    transport: data.transport,
    user: data.user.trim(),
    auth_user: data.auth_user.trim() || null,
    password: data.password,
    display_name: data.display_name.trim() || null,
  };
  const { password, ...kept } = account;
  save(STORE.account, kept);
  $("acct-name").textContent = `${account.user}@${account.server}`;
  setReg("pending", "Registering…");
  try {
    await invoke("sip_register", { account });
    settingsOpen = false;
    render();
  } catch (err) {
    setReg("err", String(err));
  }
}

// ---------- audio devices ----------

// Saved by device name, since device numbers can change between launches.
function fillSelect(sel, devices, saved) {
  sel.innerHTML = "";
  sel.add(new Option("System default", ""));
  for (const d of devices) sel.add(new Option(d.name, d.name));
  sel.value = devices.some((d) => d.name === saved) ? saved : "";
}

async function loadDevices() {
  const devices = await invoke("audio_devices");
  const saved = load(STORE.audio, {});
  fillSelect($("mic"), devices.filter((d) => d.inputs > 0), saved.mic);
  fillSelect($("speaker"), devices.filter((d) => d.outputs > 0), saved.speaker);
  $("ring-on").checked = saved.ring !== false;
  if (!$("ringtone").options.length) {
    const names = await invoke("ringtones").catch(() => []);
    names.forEach((n, i) => $("ringtone").add(new Option(n, String(i))));
  }
  $("ringtone").value = String(saved.tone ?? 0);
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
  } catch {
    toast("Couldn't switch audio device.");
  }
}

async function onAudioChange() {
  save(STORE.audio, {
    mic: $("mic").value,
    speaker: $("speaker").value,
    ring: $("ring-on").checked,
    tone: Number($("ringtone").value) || 0,
  });
  await applyAudio(true);
}

// ---------- SIP events ----------

function onSip(ev) {
  switch (ev.type) {
    case "registration":
      if (ev.registered) setReg("ok", "Registered");
      else if (ev.code >= 300 || ev.code === 0) setReg("err", `${ev.code || ""} ${ev.reason || "Not registered"}`.trim());
      else setReg("", "Not registered");
      break;
    case "incoming": {
      addCall(ev.call_id, ev.remote, true);
      if (liveCalls().length === 1) {
        focusId = ev.call_id;
        settingsOpen = false;
      }
      updateRinger();
      render();
      break;
    }
    case "call_state": {
      const call = calls.get(ev.call_id);
      if (!call) return;
      if (!(call.incoming && ev.state === "ringing")) call.state = ev.state;
      if (ev.state === "confirmed") {
        call.answered = true;
        if (!call.since) call.since = Date.now();
      }
      if (ev.state === "disconnected") endCall(call, ev.code);
      updateRinger();
      render();
      break;
    }
    case "media": {
      const call = calls.get(ev.call_id);
      if (call) {
        call.held = ev.status === "local_hold";
        render();
      }
      break;
    }
    case "transfer":
      if (ev.done) toast(ev.code >= 200 && ev.code < 300 ? "Transferred." : `Transfer failed (${ev.code} ${ev.reason}).`);
      break;
    case "log": {
      if (/Unable to open sound device|PJMEDIA_EAUD/.test(ev.line) && !onSip.audioWarned) {
        onSip.audioWarned = true;
        toast("Couldn't open the microphone or speaker. Check microphone permission and Audio settings.");
      }
      const log = $("log");
      log.textContent += ev.line + "\n";
      if (log.textContent.length > 60000) log.textContent = log.textContent.slice(-40000);
      break;
    }
  }
}

// ---------- wiring ----------

buildPad($("pad"), (d) => { $("dial").value += d; $("dial").focus(); });
buildPad($("dtmf-pad"), (d) => focusId != null && invoke("sip_dtmf", { callId: focusId, digits: d }));

$("btn-call").addEventListener("click", () => dial());
$("dial").addEventListener("keydown", (e) => e.key === "Enter" && dial());
$("btn-back").addEventListener("click", () => { $("dial").value = $("dial").value.slice(0, -1); });
$("btn-answer").addEventListener("click", () => focusId != null && answer(focusId));
$("btn-decline").addEventListener("click", () => focusId != null && hangup(focusId));
$("btn-hangup").addEventListener("click", () => focusId != null && hangup(focusId));
$("btn-mute").addEventListener("click", async () => {
  const c = calls.get(focusId);
  if (!c) return;
  await invoke("sip_mute", { callId: c.id, mute: !c.muted });
  c.muted = !c.muted;
  render();
});
$("btn-hold").addEventListener("click", () => {
  const c = calls.get(focusId);
  if (c) invoke("sip_hold", { callId: c.id, hold: !c.held });
});
$("btn-keypad").addEventListener("click", () => {
  $("xfer-panel").classList.add("hidden");
  $("btn-xfer").classList.remove("on");
  $("dtmf-pad").classList.toggle("hidden");
  $("btn-keypad").classList.toggle("on");
});
$("btn-xfer").addEventListener("click", () => {
  $("dtmf-pad").classList.add("hidden");
  $("btn-keypad").classList.remove("on");
  const open = $("xfer-panel").classList.toggle("hidden");
  $("btn-xfer").classList.toggle("on", !open);
  if (!open) $("xfer-dest").focus();
});
$("xfer-panel").addEventListener("submit", blindTransfer);
$("btn-xfer-consult").addEventListener("click", consultTransfer);

for (const b of $("tabs").querySelectorAll("button")) {
  b.addEventListener("click", () => setTab(b.dataset.tab));
}
$("btn-settings").addEventListener("click", () => {
  settingsOpen = !settingsOpen;
  if (settingsOpen) loadDevices();
  render();
});
$("acct-form").addEventListener("submit", register);
$("btn-unregister").addEventListener("click", async () => {
  try { await invoke("sip_unregister"); } catch {}
  setReg("", "Not registered");
});
$("btn-show-log").addEventListener("click", async () => {
  try {
    await invoke("show_log");
  } catch (err) {
    toast("Couldn't open the log folder: " + err);
  }
});
$("mic").addEventListener("change", onAudioChange);
$("speaker").addEventListener("change", onAudioChange);
$("ring-on").addEventListener("change", onAudioChange);
$("ringtone").addEventListener("change", onAudioChange);
$("btn-preview").addEventListener("click", () => {
  if (liveCalls().length) return toast("Can't preview during a call.");
  invoke("sip_ring", { mode: "preview", tone: Number($("ringtone").value) || 0 }).catch(() => {});
});

$("btn-clear-recents").addEventListener("click", () => {
  save(STORE.recents, []);
  save(STORE.missed, 0);
  render();
});
$("btn-add-contact").addEventListener("click", () => {
  $("contact-form").classList.toggle("hidden");
  $("contact-form").elements.name.focus();
});
$("btn-cancel-contact").addEventListener("click", () => {
  $("contact-form").reset();
  $("contact-form").classList.add("hidden");
});
$("contact-form").addEventListener("submit", saveContact);
$("contact-search").addEventListener("input", renderContacts);

// Typing digits anywhere on the keypad tab goes into the dial box.
document.addEventListener("keydown", (e) => {
  if (e.target.matches("input, select")) return;
  if (calls.size || settingsOpen || tab !== "dial") return;
  if (/^[0-9*#+]$/.test(e.key)) $("dial").value += e.key;
  else if (e.key === "Backspace") $("dial").value = $("dial").value.slice(0, -1);
  else if (e.key === "Enter") dial();
});

ticker = setInterval(tick, 1000);
fillForm();
refreshContactList();
listen("sip", (e) => onSip(e.payload));
listen("show-recents", () => setTab("recents"));
applyAudio();
if (!load(STORE.account, {}).server) settingsOpen = true;
render();

// ---------- browser preview (no Tauri) ----------

function mockInvoke(cmd, args) {
  console.log("invoke", cmd, args);
  if (cmd === "sip_register") setTimeout(() => onSip({ type: "registration", registered: true, code: 200, reason: "OK" }), 400);
  if (cmd === "sip_call") return Promise.resolve((mockInvoke.n = (mockInvoke.n || 0) + 1));
  if (cmd === "ringtones") return Promise.resolve(["Chirp", "Classic", "British", "Digital", "Chime", "Soft", "Office"]);
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

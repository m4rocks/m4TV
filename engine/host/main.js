// The Engine host (ADR 0006): castLabs Electron, spawned and supervised by the Shell. It keeps one
// hidden off-screen window for the open App, in a storage partition of its own, and talks to the
// Shell over a socket: newline-delimited JSON, commands one way, frames and live state the other.
//
//   Shell -> host   launch {session, app: {id, start_url, user_agent, script}, width, height}
//                   close, key {key}, text {text}, back, cdm_retry
//   host -> Shell   ready, frame {session, slot, width, height},
//                   state {session, can_go_back, editable},
//                   cdm {status: preparing | ready | failed, reason}
//
// A frame is the page's whole paint bitmap (BGRA), copied into `frame-<slot>.bgra` in the frames
// directory (/dev/shm on the device); the message only says which slot is new.

const { app, BrowserWindow, components, ipcMain, session } = require("electron");
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");

const { M4TV_SOCKET, M4TV_FRAMES, M4TV_DATA, M4TV_CDM } = process.env;
if (!M4TV_SOCKET || !M4TV_FRAMES) {
  console.error("m4tv-engine-host: M4TV_SOCKET and M4TV_FRAMES are set by the Shell");
  process.exit(2);
}
if (M4TV_DATA) app.setPath("userData", path.resolve(M4TV_DATA));

// An error nobody handles would otherwise stop at a dialog; the Shell starts a host that exits.
process.on("uncaughtException", (error) => {
  console.error("m4tv-engine-host:", error);
  app.exit(1);
});

// An App's video starts without anyone having clicked in the page.
app.commandLine.appendSwitch("autoplay-policy", "no-user-gesture-required");

const FRAME_RATE = 30;
// Slots rotate, so the Shell is never reading the file that is being written.
const SLOTS = 3;

let socket = null;
let page = null; // { session, window }
let slot = 0;
let editable = false;

function send(message) {
  if (socket && !socket.destroyed) socket.write(JSON.stringify(message) + "\n");
}

function sendState() {
  if (!page) return;
  send({
    t: "state",
    session: page.session,
    can_go_back: page.window.webContents.navigationHistory.canGoBack(),
    editable,
  });
}

function closePage() {
  if (!page) return;
  const { window } = page;
  page = null;
  editable = false;
  if (!window.isDestroyed()) window.destroy();
  // The App's frames go with its window.
  for (let n = 0; n < SLOTS; n++) fs.rmSync(path.join(M4TV_FRAMES, `frame-${n}.bgra`), { force: true });
}

function launch({ session: id, app: definition, width, height }) {
  closePage();
  const partition = `persist:${definition.id}`;
  const storage = session.fromPartition(partition);
  const userAgent = definition.user_agent;
  if (userAgent) {
    // Set on the storage as well as the page, so a service worker or a redirect cannot lose it.
    storage.setUserAgent(userAgent);
    storage.webRequest.onBeforeSendHeaders((details, done) => {
      done({ requestHeaders: { ...details.requestHeaders, "User-Agent": userAgent } });
    });
  }

  const window = new BrowserWindow({
    show: false,
    frame: false,
    width,
    height,
    useContentSize: true,
    webPreferences: {
      offscreen: true,
      partition,
      preload: path.join(__dirname, "preload.js"),
      backgroundThrottling: false,
    },
  });
  const contents = window.webContents;
  page = { session: id, window };
  contents.setFrameRate(FRAME_RATE);
  if (userAgent) contents.setUserAgent(userAgent);
  contents.focus();

  contents.on("paint", (_event, _dirty, image) => {
    if (!page || page.window !== window) return;
    const { width, height } = image.getSize();
    fs.writeFileSync(path.join(M4TV_FRAMES, `frame-${slot}.bgra`), image.toBitmap());
    send({ t: "frame", session: id, slot, width, height });
    slot = (slot + 1) % SLOTS;
  });
  // The TV tuning goes into every page of the App, not into its sub-frames.
  contents.on("dom-ready", () => {
    if (definition.script) contents.executeJavaScript(definition.script).catch(() => {});
  });
  contents.on("did-navigate", sendState);
  contents.on("did-navigate-in-page", sendState);
  // An App has one window: a link that wants a new one opens here.
  contents.setWindowOpenHandler(({ url }) => {
    contents.loadURL(url);
    return { action: "deny" };
  });
  contents.on("render-process-gone", () => contents.reload());
  contents.loadURL(definition.start_url, userAgent ? { userAgent } : undefined);
  sendState();
}

function key(code) {
  if (!page) return;
  const contents = page.window.webContents;
  contents.sendInputEvent({ type: "keyDown", keyCode: code });
  // Enter also produces a character, which is what activates focused page controls.
  if (code === "Return") contents.sendInputEvent({ type: "char", keyCode: "\r" });
  contents.sendInputEvent({ type: "keyUp", keyCode: code });
}

// The Widevine CDM. castLabs fetches it itself once there is a network. Where DRM cannot run (the
// Dev host on Windows) it counts as ready, unless M4TV_CDM scripts it: ready, preparing, or
// failed:<reason>.
let cdm = { status: "preparing" };

function reportCdm(status, reason) {
  cdm = { status, reason };
  send({ t: "cdm", status, reason });
}

function reasonOf(error) {
  const text = String((error && error.message) || error);
  if (/INTERNET_DISCONNECTED|NAME_NOT_RESOLVED|ENOTFOUND|ECONNREFUSED|ETIMEDOUT|offline/i.test(text)) {
    return "offline: the TV has no internet connection";
  }
  if (/\b(401|403|451)\b|blocked|forbidden/i.test(text)) return "blocked: the download was refused";
  return `failed: ${text}`;
}

function prepareCdm() {
  reportCdm("preparing");
  if (M4TV_CDM) {
    const [status, ...reason] = M4TV_CDM.split(":");
    setTimeout(() => reportCdm(status, reason.join(":") || undefined), 500);
  } else if (!components) {
    reportCdm(process.platform === "linux" ? "failed" : "ready", "this Electron has no Widevine");
  } else if (process.platform !== "linux") {
    reportCdm("ready");
  } else {
    components.whenReady().then(
      () => reportCdm("ready"),
      (error) => reportCdm("failed", reasonOf(error)),
    );
  }
}

function retryCdm() {
  if (components && components.updateAll && !M4TV_CDM) {
    reportCdm("preparing");
    // whenReady() may still hold the first failure, so a finished update is what says ready.
    components.updateAll().then(
      () => reportCdm("ready"),
      (error) => reportCdm("failed", reasonOf(error)),
    );
  } else {
    prepareCdm();
  }
}

const commands = {
  cdm_retry: retryCdm,
  launch,
  close: closePage,
  key: ({ key: code }) => key(code),
  text: ({ text }) => page && page.window.webContents.insertText(text),
  back: () => page && page.window.webContents.navigationHistory.goBack(),
};

ipcMain.on("m4tv:editable", (event, now) => {
  if (!page || event.sender !== page.window.webContents) return;
  editable = now;
  sendState();
});

// The Shell owns the lifetime: when it goes away, so does the host.
app.on("window-all-closed", () => {});

app.whenReady().then(() => {
  socket = net.connect(M4TV_SOCKET);
  socket.on("connect", () => {
    send({ t: "ready", electron: process.versions.electron });
    if (cdm.status === "preparing") prepareCdm();
    else reportCdm(cdm.status, cdm.reason);
  });
  socket.on("close", () => app.exit(0));
  socket.on("error", (error) => {
    console.error("m4tv-engine-host:", error.message);
    app.exit(1);
  });
  socket.setEncoding("utf8");
  let pending = "";
  socket.on("data", (chunk) => {
    pending += chunk;
    let end;
    while ((end = pending.indexOf("\n")) >= 0) {
      const line = pending.slice(0, end);
      pending = pending.slice(end + 1);
      try {
        const message = JSON.parse(line);
        commands[message.t]?.(message);
      } catch (error) {
        console.error("m4tv-engine-host:", error);
      }
    }
  });
});

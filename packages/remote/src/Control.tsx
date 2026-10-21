import { useEffect, useMemo, useState } from "react";
import { CommandError, createApi } from "./api/api";
import { connectLive, type Link } from "./live/live";
import type { LiveState } from "./protocol/api";
import { stepped } from "./settings/settings";
import { joinRequest, type JoinForm, type Security } from "./wifi/wifi";

const TABS = { wifi: "Wi-Fi", remote: "Remote", apps: "Apps", settings: "Settings" };

const pad = "rounded-lg bg-gray-200 p-4 text-xl active:bg-gray-300 disabled:opacity-40";

export function Control({ token, onUnpaired }: { token: string; onUnpaired: () => void }) {
  const api = useMemo(() => createApi(token), [token]);
  const [live, setLive] = useState<LiveState | null>(null);
  const [link, setLink] = useState<Link>("connecting");
  const [error, setError] = useState("");
  const [tab, setTab] = useState<"remote" | "apps" | "settings" | "wifi">("remote");

  // While the TV is setting itself up on its hotspot, the Wi-Fi tab comes first.
  const inSetup = !!live && live.setup.phase !== "off";
  useEffect(() => {
    setTab((current) => (inSetup ? "wifi" : current === "wifi" ? "remote" : current));
  }, [inSetup]);

  useEffect(() => {
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const connection = connectLive({
      url: `${scheme}://${location.host}/api/ws?token=${token}`,
      onState: setLive,
      onStatus: setLink,
    });
    const wake = () => document.visibilityState === "visible" && connection.wake();
    document.addEventListener("visibilitychange", wake);
    return () => {
      document.removeEventListener("visibilitychange", wake);
      connection.close();
    };
  }, [token]);

  // A revoked phone (or a replaced certificate) is refused at the WebSocket, which looks like an
  // unreachable TV: ask the API, which says 401, and go back to pairing.
  useEffect(() => {
    // Whatever failed before, the link is back.
    if (link === "live") setError("");
    if (link !== "offline") return;
    fetch("/api/state", { headers: { Authorization: `Bearer ${token}` } })
      .then((response) => response.status === 401 && onUnpaired())
      .catch(() => {});
  }, [link]);

  async function run(command: Promise<void>) {
    try {
      await command;
      setError("");
    } catch (e) {
      if (e instanceof CommandError && e.unpaired) return onUnpaired();
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  const key = (code: string, label: string) => (
    <button className={pad} onClick={() => run(api.key(code))}>
      {label}
    </button>
  );

  return (
    <div className="flex flex-1 flex-col gap-4">
      {link !== "live" && (
        <p role="status" className="rounded bg-yellow-100 p-3 text-yellow-900">
          {link === "offline" ? "Can't reach the TV. Reconnecting…" : "Connecting…"}
        </p>
      )}
      {error && (
        <p role="alert" className="rounded bg-red-100 p-3 text-red-800">
          {error}
        </p>
      )}

      {tab === "remote" ? (
        <div className="space-y-4">
          <div className="mx-auto grid max-w-xs grid-cols-3 gap-2">
            <span />
            {key("ArrowUp", "Up")}
            <span />
            {key("ArrowLeft", "Left")}
            {key("Enter", "OK")}
            {key("ArrowRight", "Right")}
            {key("Escape", "Back")}
            {key("ArrowDown", "Down")}
            {key("Home", "Home")}
          </div>
          <div className="grid grid-cols-3 gap-2">
            {key("MediaTrackPrevious", "Prev")}
            {key("MediaPlayPause", "Play/Pause")}
            {key("MediaTrackNext", "Next")}
            {key("AudioVolumeDown", "Vol −")}
            {key("AudioVolumeMute", live?.muted ? "Unmute" : "Mute")}
            {key("AudioVolumeUp", "Vol +")}
          </div>
          {live?.editable && <TextBox onSend={(text) => run(api.text(text))} />}
        </div>
      ) : tab === "apps" ? (
        <Apps live={live} onLaunch={(id) => run(api.launch(id))} onOrder={(ids) => run(api.order(ids))} />
      ) : tab === "wifi" ? (
        <Wifi live={live} onJoin={(form) => run(api.join(joinRequest(form)))} />
      ) : (
        <Settings live={live} onChange={(key, value) => run(api.setting(key, value))} />
      )}

      <nav className="mt-auto flex gap-2" aria-label="Sections">
        {(inSetup ? (["wifi", "remote", "apps", "settings"] as const) : (["remote", "apps", "settings"] as const)).map((name) => (
          <button
            key={name}
            aria-current={tab === name}
            className={`flex-1 rounded p-3 ${tab === name ? "bg-blue-600 text-white" : "bg-gray-200"}`}
            onClick={() => setTab(name)}
          >
            {TABS[name]}
          </button>
        ))}
      </nav>
    </div>
  );
}

function TextBox({ onSend }: { onSend: (text: string) => Promise<void> }) {
  const [text, setText] = useState("");
  return (
    <form
      className="flex gap-2"
      onSubmit={async (e) => {
        e.preventDefault();
        await onSend(text);
        setText("");
      }}
    >
      <input
        className="min-w-0 flex-1 rounded border p-3 text-lg"
        aria-label="Text for the TV"
        placeholder="Type here"
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <button className="rounded bg-blue-600 px-4 py-3 text-white">Send</button>
    </form>
  );
}

function Apps({
  live,
  onLaunch,
  onOrder,
}: {
  live: LiveState | null;
  onLaunch: (id: string) => void;
  onOrder: (ids: string[]) => void;
}) {
  if (!live) return <p>Waiting for the TV…</p>;
  const ids = live.apps.map((app) => app.id);
  const move = (from: number, to: number) => {
    const next = [...ids];
    next.splice(to, 0, ...next.splice(from, 1));
    onOrder(next);
  };
  return (
    <ul className="space-y-2">
      {live.apps.map((app, i) => (
        <li key={app.id} className="flex items-center gap-2">
          <button className="flex-1 rounded bg-gray-200 p-3 text-left text-lg" onClick={() => onLaunch(app.id)}>
            {app.name}
          </button>
          <button className={pad} aria-label={`Move ${app.name} up`} disabled={i === 0} onClick={() => move(i, i - 1)}>
            ↑
          </button>
          <button
            className={pad}
            aria-label={`Move ${app.name} down`}
            disabled={i === ids.length - 1}
            onClick={() => move(i, i + 1)}
          >
            ↓
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Built from the Shell's declaration, so a new setting needs no code here. */
function Settings({
  live,
  onChange,
}: {
  live: LiveState | null;
  onChange: (key: string, value: boolean | number) => void;
}) {
  if (!live) return <p>Waiting for the TV…</p>;
  return (
    <ul className="space-y-2">
      {live.settings.map((setting) => (
        <li key={setting.key} className="flex items-center gap-2">
          <span className="flex-1 text-lg">{setting.label}</span>
          {setting.kind.type === "toggle" ? (
            <button
              className={pad}
              role="switch"
              aria-checked={setting.value as boolean}
              onClick={() => onChange(setting.key, stepped(setting, 1))}
            >
              {setting.value ? "On" : "Off"}
            </button>
          ) : (
            <>
              <button
                className={pad}
                aria-label={`${setting.label} down`}
                onClick={() => onChange(setting.key, stepped(setting, -1))}
              >
                −
              </button>
              <span className="w-12 text-center text-lg">{String(setting.value)}</span>
              <button
                className={pad}
                aria-label={`${setting.label} up`}
                onClick={() => onChange(setting.key, stepped(setting, 1))}
              >
                +
              </button>
            </>
          )}
        </li>
      ))}
    </ul>
  );
}

/** Picking a Wi-Fi for the TV while it sets itself up on its hotspot. */
function Wifi({ live, onJoin }: { live: LiveState | null; onJoin: (form: JoinForm) => void }) {
  const [form, setForm] = useState<JoinForm | null>(null);
  if (!live) return <p>Waiting for the TV…</p>;
  const setup = live.setup;
  if (setup.phase === "off") return <p>The TV is on a network.</p>;
  if (setup.phase === "joining") return <p role="status">The TV is joining {setup.ssid}…</p>;
  if (setup.phase === "joined") {
    return (
      <p role="status">
        The TV joined the network. Join the same Wi-Fi on this phone, then scan the new QR code on the TV
        (or open https://{setup.address}).
      </p>
    );
  }
  const start = (ssid: string, security: Security, hidden: boolean) =>
    setForm({ ssid, security, password: "", hidden });
  if (form) {
    return (
      <form
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          onJoin(form);
        }}
      >
        <h2 className="text-lg font-bold">{form.hidden ? "Hidden network" : form.ssid}</h2>
        {form.hidden && (
          <>
            <input
              className="w-full rounded border p-3 text-lg"
              aria-label="Network name"
              placeholder="Network name"
              maxLength={32}
              value={form.ssid}
              onChange={(e) => setForm({ ...form, ssid: e.target.value })}
            />
            <select
              className="w-full rounded border p-3 text-lg"
              aria-label="Security"
              value={form.security}
              onChange={(e) => setForm({ ...form, security: e.target.value as Security })}
            >
              <option value="wpa2">WPA2</option>
              <option value="wpa3">WPA3</option>
              <option value="open">Open</option>
            </select>
          </>
        )}
        {form.security !== "open" && (
          <input
            className="w-full rounded border p-3 text-lg"
            type="password"
            aria-label="Password"
            placeholder="Password"
            autoComplete="off"
            value={form.password}
            onChange={(e) => setForm({ ...form, password: e.target.value })}
          />
        )}
        <div className="flex gap-2">
          <button type="button" className={pad} onClick={() => setForm(null)}>
            Back
          </button>
          <button className="flex-1 rounded bg-blue-600 p-4 text-xl text-white">Join</button>
        </div>
      </form>
    );
  }
  return (
    <div className="space-y-2">
      {setup.reason && (
        <p role="alert" className="rounded bg-red-100 p-3 text-red-800">
          Could not join: {setup.reason}
        </p>
      )}
      <ul className="space-y-2">
        {setup.networks.map((network) => (
          <li key={network.ssid}>
            <button
              className="w-full rounded bg-gray-200 p-3 text-left text-lg"
              onClick={() => start(network.ssid, network.security as Security, false)}
            >
              {network.ssid}
              {network.security !== "open" && " 🔒"}
            </button>
          </li>
        ))}
      </ul>
      <button className="w-full rounded bg-gray-200 p-3 text-left text-lg" onClick={() => start("", "wpa2", true)}>
        Other network…
      </button>
    </div>
  );
}

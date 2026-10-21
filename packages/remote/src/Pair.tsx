import { useEffect, useState } from "react";
import { pairByPin, pairBySecret, pairOnHotspot, parseLink, type Post } from "./pairing/pair";

const post: Post = async (path, body) => {
  const response = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return { status: response.status, body: await response.json().catch(() => null) };
};

const NAME = "Phone";

export function Pair({ onPaired }: { onPaired: (token: string) => void }) {
  const link = parseLink(location.hash);
  const [pin, setPin] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function run(pairing: Promise<string>) {
    setBusy(true);
    setError("");
    try {
      const token = await pairing;
      // The secret is spent: keep it out of the address bar and the history.
      history.replaceState(null, "", location.pathname);
      onPaired(token);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }

  // The Setup hotspot serves the Remote over plain HTTP: joining it with its password is the proof.
  const onHotspot = location.protocol === "http:";
  const [fingerprint, setFingerprint] = useState("");

  useEffect(() => {
    // Once, on the scan: a retry is the PIN form (or the button on the hotspot) below.
    if (link) void run(pairBySecret(post, link.secret, NAME));
    else if (onHotspot) {
      // Pairing here has no PIN to type, so it waits for a tap rather than adding a phone on
      // every page load.
      fetch("/api/hotspot")
        .then((response) => response.json())
        .then((body) => setFingerprint(body.fingerprint))
        .catch(() => {});
    }
  }, []);

  if (onHotspot && !link) {
    return (
      <section className="space-y-4">
        <p>Setting up this TV over its own Wi-Fi.</p>
        {fingerprint && (
          <p className="break-all text-sm text-gray-600">
            Certificate fingerprint, to compare with the TV later: {fingerprint}
          </p>
        )}
        {error && (
          <p role="alert" className="rounded bg-red-100 p-3 text-red-800">
            {error}
          </p>
        )}
        <button
          className="rounded bg-blue-600 px-4 py-3 text-white disabled:opacity-50"
          disabled={busy}
          onClick={() => run(pairOnHotspot(post, NAME))}
        >
          Pair with this TV
        </button>
      </section>
    );
  }

  return (
    <section className="space-y-4">
      {link ? (
        <p>
          Pairing with the TV…
          {link.fp && (
            <span className="mt-2 block break-all text-sm text-gray-600">
              Certificate fingerprint, as shown on the TV: {link.fp}
            </span>
          )}
        </p>
      ) : (
        <p>Scan the QR code on the TV, or enter the PIN it shows.</p>
      )}
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void run(pairByPin(post, pin.trim(), NAME));
        }}
      >
        <input
          className="min-w-0 flex-1 rounded border p-3 text-lg"
          inputMode="numeric"
          autoComplete="off"
          aria-label="PIN"
          placeholder="PIN"
          value={pin}
          onChange={(e) => setPin(e.target.value)}
        />
        <button className="rounded bg-blue-600 px-4 py-3 text-white disabled:opacity-50" disabled={busy || !pin.trim()}>
          Pair
        </button>
      </form>
      {error && (
        <p role="alert" className="rounded bg-red-100 p-3 text-red-800">
          {error}
        </p>
      )}
    </section>
  );
}

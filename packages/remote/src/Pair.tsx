import { useEffect, useState } from "react";
import { pairByPin, pairBySecret, parseLink, type Post } from "./pairing/pair";

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

  useEffect(() => {
    // Once, on the scan: a retry is the PIN form below.
    if (link) void run(pairBySecret(post, link.secret, NAME));
  }, []);

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

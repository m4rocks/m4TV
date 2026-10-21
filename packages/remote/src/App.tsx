import { useState } from "react";
import { Control } from "./Control";
import { Pair } from "./Pair";

const KEY = "m4tv-token";

// Storage can throw (private windows, blocked site data): the phone just pairs again then.
function load(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

function save(token: string | null) {
  try {
    if (token) localStorage.setItem(KEY, token);
    else localStorage.removeItem(KEY);
  } catch {}
}

export function App() {
  const [token, setToken] = useState(load);
  const change = (next: string | null) => {
    save(next);
    setToken(next);
  };
  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col p-4">
      <h1 className="mb-4 text-2xl font-bold">m4TV Remote</h1>
      {token ? <Control token={token} onUnpaired={() => change(null)} /> : <Pair onPaired={change} />}
    </main>
  );
}

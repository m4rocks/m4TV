import type { WifiJoinRequest } from "../protocol/api";

export const UNREACHABLE = "Can't reach the TV";

const TIMEOUT = 5000;

/** A command the TV refused or could not take. `unpaired` means this phone's token is no good. */
export class CommandError extends Error {
  constructor(
    message: string,
    readonly unpaired = false,
  ) {
    super(message);
  }
}

export function createApi(token: string, fetcher: typeof fetch = fetch) {
  async function call(method: string, path: string, body: unknown) {
    // A phone that slept can leave a half-dead link that never answers: give up after a while.
    // (AbortSignal.timeout is iOS 16+; ADR 0004 targets iOS 14.)
    const abort = new AbortController();
    const timer = setTimeout(() => abort.abort(), TIMEOUT);
    let response: Response;
    try {
      response = await fetcher(`/api${path}`, {
        method,
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: abort.signal,
      });
    } catch {
      throw new CommandError(UNREACHABLE);
    } finally {
      clearTimeout(timer);
    }
    if (response.ok) return;
    const reason = (await response.json().catch(() => null))?.error;
    throw new CommandError(reason ?? `The TV refused: ${response.status}`, response.status === 401);
  }
  return {
    key: (code: string) => call("POST", "/key", { code }),
    launch: (app: string) => call("POST", "/launch", { app }),
    order: (apps: string[]) => call("PUT", "/apps/order", { apps }),
    retryCdm: () => call("POST", "/engine/cdm/retry", {}),
    text: (text: string) => call("POST", "/text", { text }),
    join: (request: WifiJoinRequest) => call("POST", "/wifi/join", request),
    step: (key: string, direction: number) =>
      call("POST", `/settings/${encodeURIComponent(key)}/step`, { direction }),
  };
}

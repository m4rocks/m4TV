import type { WifiJoinRequest } from "../protocol/api";

export const UNREACHABLE = "Can't reach the TV";

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
    let response: Response;
    try {
      response = await fetcher(`/api${path}`, {
        method,
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    } catch {
      throw new CommandError(UNREACHABLE);
    }
    if (response.ok) return;
    const reason = (await response.json().catch(() => null))?.error;
    throw new CommandError(reason ?? `The TV refused: ${response.status}`, response.status === 401);
  }
  return {
    key: (code: string) => call("POST", "/key", { code }),
    launch: (app: string) => call("POST", "/launch", { app }),
    order: (apps: string[]) => call("PUT", "/apps/order", { apps }),
    text: (text: string) => call("POST", "/text", { text }),
    join: (request: WifiJoinRequest) => call("POST", "/wifi/join", request),
    setting: (key: string, value: boolean | number) =>
      call("PUT", `/settings/${encodeURIComponent(key)}`, { value }),
  };
}

import type { LiveState } from "../protocol/api";

export type Link = "connecting" | "live" | "offline";

const MAX_WAIT = 10_000;

/** Keeps a WebSocket to the TV open for the live state, reconnecting with a capped backoff. */
export function connectLive(options: {
  url: string;
  onState: (state: LiveState) => void;
  onStatus: (link: Link) => void;
  Socket?: typeof WebSocket;
}) {
  const { url, onState, onStatus, Socket = WebSocket } = options;
  let socket: WebSocket | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let failures = 0;
  let stopped = false;

  function open() {
    clearTimeout(timer);
    onStatus("connecting");
    const mine = new Socket(url);
    socket = mine;
    mine.onopen = () => {
      failures = 0;
      onStatus("live");
    };
    mine.onmessage = (event) => onState(JSON.parse(event.data));
    mine.onclose = () => {
      if (stopped || socket !== mine) return;
      socket = null;
      onStatus("offline");
      timer = setTimeout(open, Math.min(1000 * 2 ** failures++, MAX_WAIT));
    };
  }

  function drop() {
    const old = socket;
    socket = null;
    if (old) old.onclose = null;
    old?.close();
  }

  open();
  return {
    /** The page came back (a phone that slept): a socket that is not provably fresh is replaced. */
    wake() {
      if (stopped || (socket && socket.readyState === Socket.CONNECTING)) return;
      drop();
      open();
    },
    close() {
      stopped = true;
      clearTimeout(timer);
      drop();
    },
  };
}

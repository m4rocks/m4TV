import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { connectLive } from "./live";

class FakeSocket {
  static CONNECTING = 0;
  static all: FakeSocket[] = [];
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  closed = false;
  constructor(public url: string) {
    FakeSocket.all.push(this);
  }
  close() {
    this.closed = true;
    this.onclose?.();
  }
}

beforeEach(() => {
  FakeSocket.all = [];
  vi.useFakeTimers();
});
afterEach(() => vi.useRealTimers());

function opened(socket: FakeSocket) {
  socket.readyState = 1;
  socket.onopen!();
}

function start() {
  const states: unknown[] = [];
  const statuses: string[] = [];
  const live = connectLive({
    url: "wss://tv/api/ws?token=t",
    onState: (s) => states.push(s),
    onStatus: (s) => statuses.push(s),
    Socket: FakeSocket as any,
  });
  return { live, states, statuses };
}

test("pushed states reach the page and the link reports live", () => {
  const { states, statuses } = start();
  const socket = FakeSocket.all[0];
  opened(socket);
  socket.onmessage!({ data: JSON.stringify({ screen: "home" }) });
  expect(states).toEqual([{ screen: "home" }]);
  expect(statuses).toEqual(["connecting", "live"]);
});

test("a dropped link is offline and retried, backing off up to a cap", () => {
  const { statuses } = start();
  FakeSocket.all[0].onclose!();
  expect(statuses[statuses.length - 1]).toBe("offline");
  vi.advanceTimersByTime(1000);
  expect(FakeSocket.all).toHaveLength(2);
  FakeSocket.all[1].onclose!();
  vi.advanceTimersByTime(1000);
  expect(FakeSocket.all).toHaveLength(2); // the second wait is longer
  vi.advanceTimersByTime(1000);
  expect(FakeSocket.all).toHaveLength(3);
  for (let i = 0; i < 10; i++) {
    FakeSocket.all[FakeSocket.all.length - 1].onclose!();
    vi.advanceTimersByTime(10_000);
  }
  expect(FakeSocket.all.length).toBeGreaterThan(10); // never gives up, never waits over 10 s
});

test("waking the phone reconnects at once instead of waiting out the backoff", () => {
  const { live } = start();
  FakeSocket.all[0].onclose!();
  live.wake();
  expect(FakeSocket.all).toHaveLength(2);
  live.wake(); // already connecting: no second socket
  expect(FakeSocket.all).toHaveLength(2);
});

test("a socket that opened and then went quiet is replaced on wake", () => {
  const { live } = start();
  opened(FakeSocket.all[0]);
  live.wake(); // the server pings a quiet phone; a slept phone's socket may be dead
  expect(FakeSocket.all[0].closed).toBe(true);
  expect(FakeSocket.all).toHaveLength(2);
});

test("closing stops it for good", () => {
  const { live } = start();
  live.close();
  vi.advanceTimersByTime(60_000);
  expect(FakeSocket.all).toHaveLength(1);
});

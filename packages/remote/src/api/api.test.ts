import { expect, test } from "vitest";
import { createApi, UNREACHABLE } from "./api";

function fetchStub(status: number, body?: unknown) {
  const calls: { url: string; init: RequestInit }[] = [];
  const fetch = async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    return new Response(body === undefined ? null : JSON.stringify(body), { status });
  };
  return { fetch: fetch as typeof globalThis.fetch, calls };
}

test("commands carry the token and their JSON body", async () => {
  const { fetch, calls } = fetchStub(204);
  await createApi("t0ken", fetch).key("ArrowUp");
  expect(calls[0].url).toBe("/api/key");
  expect(calls[0].init.method).toBe("POST");
  expect((calls[0].init.headers as Record<string, string>).Authorization).toBe("Bearer t0ken");
  expect(calls[0].init.body).toBe('{"code":"ArrowUp"}');
});

test("reordering is a PUT of the whole order", async () => {
  const { fetch, calls } = fetchStub(204);
  await createApi("t", fetch).order(["b", "a"]);
  expect(calls[0].init.method).toBe("PUT");
  expect(calls[0].url).toBe("/api/apps/order");
});

test("a refused command throws the TV's reason", async () => {
  const { fetch } = fetchStub(409, { error: "no editable field is focused" });
  await expect(createApi("t", fetch).text("hi")).rejects.toThrow("no editable field is focused");
});

test("an unreachable TV throws a plain error", async () => {
  const fetch = (async () => {
    throw new TypeError("Failed to fetch");
  }) as typeof globalThis.fetch;
  await expect(createApi("t", fetch).launch("x")).rejects.toThrow(UNREACHABLE);
});

test("a revoked token is reported as such", async () => {
  const { fetch } = fetchStub(401, { error: "pair this phone first" });
  await expect(createApi("t", fetch).key("Enter")).rejects.toMatchObject({ unpaired: true });
});

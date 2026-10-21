import { expect, test } from "vitest";
import { moved, setupMessage, tabAfter, tabsFor } from "./screens";

test("the Wi-Fi tab exists, and comes first, only while the TV is setting itself up", () => {
  expect(tabsFor(false)).toEqual(["remote", "apps", "settings"]);
  expect(tabsFor(true)).toEqual(["wifi", "remote", "apps", "settings"]);
});

test("setup opens the Wi-Fi tab and ends back on the Remote, leaving other tabs alone", () => {
  expect(tabAfter("remote", true)).toBe("wifi");
  expect(tabAfter("wifi", false)).toBe("remote");
  expect(tabAfter("apps", false)).toBe("apps");
});

test("moving an App up or down gives the whole new order and leaves the old one alone", () => {
  const ids = ["a", "b", "c"];
  expect(moved(ids, 0, 1)).toEqual(["b", "a", "c"]);
  expect(moved(ids, 2, 1)).toEqual(["a", "c", "b"]);
  expect(ids).toEqual(["a", "b", "c"]);
});

test("joining and joined say what to do next, and the other phases say nothing", () => {
  expect(setupMessage({ phase: "joining", ssid: "Home" })).toBe("The TV is joining Home…");
  expect(setupMessage({ phase: "joined", address: "10.0.0.7" })).toContain("https://10.0.0.7");
  expect(setupMessage({ phase: "hotspot", reason: null, networks: [] })).toBeNull();
  expect(setupMessage({ phase: "off" })).toBeNull();
});

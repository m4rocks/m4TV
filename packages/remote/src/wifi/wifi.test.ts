import { expect, test } from "vitest";
import { joinRequest } from "./wifi";

test("a WPA network sends its password, kept exactly as typed", () => {
  expect(joinRequest({ ssid: "Home", security: "wpa2", password: " hunter22 ", hidden: false })).toEqual({
    ssid: "Home",
    security: "wpa2",
    password: " hunter22 ",
    hidden: false,
  });
});

test("an open network sends no password even if one was typed before switching", () => {
  expect(joinRequest({ ssid: "Cafe", security: "open", password: "left over", hidden: false })).toEqual({
    ssid: "Cafe",
    security: "open",
    password: null,
    hidden: false,
  });
});

test("a hidden network is asked for by name", () => {
  expect(joinRequest({ ssid: "Secret", security: "wpa3", password: "hunter22", hidden: true }).hidden).toBe(true);
});

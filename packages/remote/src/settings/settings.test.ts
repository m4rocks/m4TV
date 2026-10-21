import { expect, test } from "vitest";
import type { SettingState } from "../protocol/api";
import { stepped } from "./settings";

const volume: SettingState = {
  key: "volume",
  label: "Volume",
  kind: { type: "range", min: 0, max: 100, step: 5 },
  value: 50,
};
const mute: SettingState = { key: "mute", label: "Mute", kind: { type: "toggle" }, value: false };

test("a range moves by its step and stops at its ends", () => {
  expect(stepped(volume, 1)).toBe(55);
  expect(stepped(volume, -1)).toBe(45);
  expect(stepped({ ...volume, value: 100 }, 1)).toBe(100);
  expect(stepped({ ...volume, value: 0 }, -1)).toBe(0);
});

test("a toggle flips either way", () => {
  expect(stepped(mute, 1)).toBe(true);
  expect(stepped({ ...mute, value: true }, -1)).toBe(false);
});

import type { SettingState } from "../protocol/api";

/** The value one step up (`direction > 0`) or down, as the Shell's Settings screen steps it. */
export function stepped(setting: SettingState, direction: number): boolean | number {
  const { kind, value } = setting;
  if (kind.type === "toggle") return !value;
  return Math.min(kind.max, Math.max(kind.min, (value as number) + Math.sign(direction) * kind.step));
}

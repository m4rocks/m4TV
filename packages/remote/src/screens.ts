import type { SettingState, SetupView } from "./protocol/api";

export type Tab = "wifi" | "remote" | "apps" | "settings";

/** The Wi-Fi tab is for the TV's setup on its hotspot, so it exists, first, only then. */
export function tabsFor(inSetup: boolean): Tab[] {
  return inSetup ? ["wifi", "remote", "apps", "settings"] : ["remote", "apps", "settings"];
}

/** Setup opens the Wi-Fi tab and, when it ends, a phone left on it goes back to the Remote. */
export function tabAfter(current: Tab, inSetup: boolean): Tab {
  if (inSetup) return "wifi";
  return current === "wifi" ? "remote" : current;
}

/** The order after moving the App at `from` to `to`. */
export function moved(ids: string[], from: number, to: number): string[] {
  const next = [...ids];
  next.splice(to, 0, ...next.splice(from, 1));
  return next;
}

/** The settings grouped by category, in the order the declaration first lists each one. */
export function byCategory(settings: SettingState[]): [string, SettingState[]][] {
  const groups = new Map<string, SettingState[]>();
  for (const setting of settings) {
    groups.set(setting.category, [...(groups.get(setting.category) ?? []), setting]);
  }
  return [...groups];
}

/** What to tell the phone while the TV joins a Wi-Fi or once it has, if anything. */
export function setupMessage(setup: SetupView): string | null {
  if (setup.phase === "joining") return `The TV is joining ${setup.ssid}…`;
  if (setup.phase === "joined") {
    return `The TV joined the network. Join the same Wi-Fi on this phone, then scan the new QR code on the TV (or open https://${setup.address}).`;
  }
  return null;
}

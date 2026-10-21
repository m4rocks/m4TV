import type { WifiJoinRequest } from "../protocol/api";

export type Security = "open" | "wpa2" | "wpa3";

export type JoinForm = { ssid: string; security: Security; password: string; hidden: boolean };

/** What the form sends. The TV checks it again; an open network never carries a password. */
export function joinRequest(form: JoinForm): WifiJoinRequest {
  return {
    ssid: form.ssid,
    security: form.security,
    password: form.security === "open" ? null : form.password,
    hidden: form.hidden,
  };
}

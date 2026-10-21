// Generated from the Rust types in crates/shell/src/remote. Do not edit: run
// `UPDATE_TS=1 cargo test -p m4tv-shell typescript`.

export type AppSummary = { id: string, name: string, };

export type CdmView = { "status": "preparing" } | { "status": "ready" } | { "status": "failed", reason: string, };

export type LiveState = { 
/**
 * "home", "app" or "settings".
 */
screen: string, 
/**
 * The open App, if there is one.
 */
app: string | null, 
/**
 * The focused tile on Home, or row on Settings.
 */
focus: number, 
/**
 * The Apps in Home's order.
 */
apps: Array<AppSummary>, volume: number, muted: boolean, 
/**
 * Whether the open App has an editable field focused.
 */
editable: boolean, 
/**
 * Every declared setting, in the declaration's order: the Remote builds its screens from this.
 */
settings: Array<SettingState>, 
/**
 * Whether the Engine's video playback is ready, and if not, why.
 */
cdm: CdmView, 
/**
 * Whether the TV is setting itself up on the Setup hotspot, and how that is going.
 */
setup: SetupView, 
/**
 * What the last device action or failed save said, until the next key: a phone cannot see
 * the TV's screen, so a save that failed would otherwise go unnoticed.
 */
notice: string | null, };

export type PairRequest = { pin: string, name: string, };

export type ProveRequest = { challenge: string, nonce: string, proof: string, name: string, };

export type KeyRequest = { code: string, };

export type LaunchRequest = { app: string, };

export type OrderRequest = { apps: Array<string>, };

export type TextRequest = { text: string, };

export type SettingState = { key: string, label: string, 
/**
 * The group the setting is listed under.
 */
category: string, kind: SettingKind, value: boolean | number, };

export type SettingKind = { "type": "range", min: number, max: number, step: number, } | { "type": "toggle" } | { "type": "choice", options: Array<string>, };

export type SettingRequest = { value: boolean | number, };

export type StepRequest = { 
/**
 * 1 or -1.
 */
direction: number, };

export type SetupView = { "phase": "off" } | { "phase": "hotspot", reason: string | null, networks: Array<WifiView>, } | { "phase": "joining", ssid: string, } | { "phase": "joined", address: string, };

export type WifiView = { ssid: string, 
/**
 * "open", "wpa2" or "wpa3".
 */
security: string, };

export type WifiJoinRequest = { ssid: string, password: string | null, 
/**
 * "open", "wpa2" or "wpa3".
 */
security: string, hidden: boolean, };

export type HotspotPairRequest = { name: string, };


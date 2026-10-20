// Generated from the Rust types in crates/shell/src/remote. Do not edit: run
// `UPDATE_TS=1 cargo test -p m4tv-shell typescript`.

export type AppSummary = { id: string, name: string, };

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
editable: boolean, };

export type PairRequest = { pin: string, name: string, };

export type ProveRequest = { challenge: string, nonce: string, proof: string, name: string, };

export type KeyRequest = { code: string, };

export type LaunchRequest = { app: string, };

export type OrderRequest = { apps: Array<string>, };

export type TextRequest = { text: string, };


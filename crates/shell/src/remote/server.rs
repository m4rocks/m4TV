//! The Remote server's HTTPS API. Everything but Pairing needs a paired phone's token.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        Query, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{
        HeaderMap, StatusCode, Uri,
        header::{AUTHORIZATION, CONTENT_TYPE, HOST},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{mpsc::UnboundedSender, watch};
use ts_rs::TS;

use super::{Command, LiveState, PairError, Pairings};
use crate::shell::Key;

/// How often an open WebSocket checks that its phone is still paired and still there.
const RECHECK: Duration = Duration::from_secs(1);
/// A quiet phone is pinged after this long, and dropped after `DEAD_AFTER`: one that slept or
/// left the Wi-Fi never closes its socket, and would hide the Pairing Overlay for good.
const PING_AFTER: Duration = Duration::from_secs(5);
const DEAD_AFTER: Duration = Duration::from_secs(15);

/// The Remote web app (`packages/remote`, built by build.rs): read from disk in a debug build,
/// compiled in for release.
#[derive(RustEmbed)]
#[folder = "../../packages/remote/dist/"]
#[exclude = ".built"]
struct Web;

#[derive(Clone)]
pub(super) struct Shared {
    pub pairings: Pairings,
    pub commands: UnboundedSender<Command>,
    pub state: Arc<watch::Sender<LiveState>>,
    pub connections: Arc<AtomicUsize>,
}

pub(super) fn router(shared: Shared) -> Router {
    let api = Router::new()
        .route("/state", get(state))
        .route("/key", post(key))
        .route("/launch", post(launch))
        .route("/apps/order", put(order))
        .route("/text", post(text))
        .layer(middleware::from_fn_with_state(shared.clone(), authenticate));
    Router::new()
        .route("/api/pair", post(pair))
        .route("/api/pair/challenge", post(challenge))
        .route("/api/pair/prove", post(prove))
        .route("/api/ws", get(ws))
        .nest("/api", api)
        .fallback_service(get(web))
        .with_state(shared)
}

/// Serves the Remote web app. Its `index.html` is also the landing page for a browser that cannot
/// run the app, so it is told the address the viewer used.
async fn web(headers: HeaderMap, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let Some(file) = Web::get(path) else { return StatusCode::NOT_FOUND.into_response() };
    if path != "index.html" {
        return ([(CONTENT_TYPE, file.metadata.mimetype().to_string())], file.data.into_owned())
            .into_response();
    }
    // The Host header is the viewer's own input, so only a plain host and port go into the page.
    let host = headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .filter(|host| host.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c)))
        .unwrap_or(super::HOST);
    let page =
        String::from_utf8_lossy(&file.data).replace("{{ADDRESS}}", &format!("https://{host}/"));
    ([(CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
}

fn refuse(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")
}

async fn authenticate(State(shared): State<Shared>, request: Request, next: Next) -> Response {
    if bearer(request.headers()).is_some_and(|token| shared.pairings.check(token)) {
        next.run(request).await
    } else {
        refuse(StatusCode::UNAUTHORIZED, "pair this phone first")
    }
}

#[derive(Deserialize, TS)]
struct PairRequest {
    pin: String,
    #[serde(default)]
    name: String,
}

/// What a phone is called in Settings.
fn phone_name(requested: &str) -> String {
    let name = requested.trim().chars().take(40).collect::<String>();
    if name.is_empty() { "Phone".to_string() } else { name }
}

async fn pair(State(shared): State<Shared>, Json(request): Json<PairRequest>) -> Response {
    let name = phone_name(&request.name);
    match shared.pairings.redeem(&request.pin, &name) {
        Ok(token) => Json(json!({ "token": token })).into_response(),
        Err(PairError::WrongPin) => refuse(StatusCode::FORBIDDEN, "wrong PIN"),
        Err(PairError::Save(_)) => {
            refuse(StatusCode::INTERNAL_SERVER_ERROR, "could not save the pairing")
        }
    }
}

async fn challenge(State(shared): State<Shared>) -> Response {
    match shared.pairings.challenge() {
        Ok(challenge) => Json(json!({ "challenge": challenge })).into_response(),
        Err(_) => refuse(StatusCode::FORBIDDEN, "not pairing"),
    }
}

#[derive(Deserialize, TS)]
struct ProveRequest {
    challenge: String,
    nonce: String,
    proof: String,
    #[serde(default)]
    name: String,
}

async fn prove(State(shared): State<Shared>, Json(request): Json<ProveRequest>) -> Response {
    let name = phone_name(&request.name);
    match shared.pairings.prove(&request.challenge, &request.nonce, &request.proof, &name) {
        Ok(proof) => Json(json!({ "proof": proof })).into_response(),
        Err(PairError::WrongPin) => refuse(StatusCode::FORBIDDEN, "wrong secret"),
        Err(PairError::Save(_)) => {
            refuse(StatusCode::INTERNAL_SERVER_ERROR, "could not save the pairing")
        }
    }
}

async fn state(State(shared): State<Shared>) -> Json<LiveState> {
    Json(shared.state.borrow().clone())
}

/// Hands the Shell a command. The Shell is gone only when the process is ending.
fn send(shared: &Shared, command: Command) -> Response {
    match shared.commands.send(command) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => refuse(StatusCode::SERVICE_UNAVAILABLE, "the TV is shutting down"),
    }
}

#[derive(Deserialize, TS)]
struct KeyRequest {
    code: String,
}

async fn key(State(shared): State<Shared>, Json(request): Json<KeyRequest>) -> Response {
    match Key::from_code(&request.code) {
        Some(key) => send(&shared, Command::Key(key)),
        None => refuse(StatusCode::BAD_REQUEST, "unknown key code"),
    }
}

#[derive(Deserialize, TS)]
struct LaunchRequest {
    app: String,
}

async fn launch(State(shared): State<Shared>, Json(request): Json<LaunchRequest>) -> Response {
    if !shared.state.borrow().apps.iter().any(|app| app.id == request.app) {
        return refuse(StatusCode::NOT_FOUND, "no such App");
    }
    send(&shared, Command::Launch(request.app))
}

#[derive(Deserialize, TS)]
struct OrderRequest {
    apps: Vec<String>,
}

async fn order(State(shared): State<Shared>, Json(request): Json<OrderRequest>) -> Response {
    let mut wanted = request.apps.clone();
    let mut have: Vec<String> = shared.state.borrow().apps.iter().map(|a| a.id.clone()).collect();
    wanted.sort();
    have.sort();
    if wanted != have {
        return refuse(StatusCode::BAD_REQUEST, "the order must list every App once");
    }
    send(&shared, Command::Order(request.apps))
}

#[derive(Deserialize, TS)]
struct TextRequest {
    text: String,
}

async fn text(State(shared): State<Shared>, Json(request): Json<TextRequest>) -> Response {
    if !shared.state.borrow().editable {
        return refuse(StatusCode::CONFLICT, "no editable field is focused");
    }
    send(&shared, Command::Text(request.text))
}

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

/// A browser cannot set headers on a WebSocket, so its token travels in the query.
async fn ws(
    State(shared): State<Shared>,
    Query(query): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !shared.pairings.check(&query.token) {
        return refuse(StatusCode::UNAUTHORIZED, "pair this phone first");
    }
    upgrade.on_upgrade(move |socket| push_state(socket, shared, query.token))
}

/// Counts an open Remote for as long as it lives.
struct Connection(Arc<AtomicUsize>);

impl Connection {
    fn open(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Sends the live state now and on every change, until the phone leaves or is revoked.
async fn push_state(mut socket: WebSocket, shared: Shared, token: String) {
    let _open = Connection::open(&shared.connections);
    let mut changes = shared.state.subscribe();
    let mut recheck = tokio::time::interval(RECHECK);
    let mut heard = Instant::now();
    loop {
        let text = serde_json::to_string(&*changes.borrow_and_update()).expect("state is JSON");
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                changed = changes.changed() => {
                    if changed.is_err() { return }
                    break;
                }
                _ = recheck.tick() => {
                    if !shared.pairings.check(&token) { return }
                    let quiet = heard.elapsed();
                    if quiet > DEAD_AFTER { return }
                    if quiet > PING_AFTER && socket.send(Message::Ping(Bytes::new())).await.is_err() {
                        return;
                    }
                }
                incoming = socket.recv() => match incoming {
                    None | Some(Err(_) | Ok(Message::Close(_))) => return,
                    Some(Ok(_)) => heard = Instant::now(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use ts_rs::{Config, TS};

    use super::*;
    use crate::remote::AppSummary;

    /// The TypeScript types the Remote web app uses, generated from the Rust ones. Stale types
    /// fail this test; `UPDATE_TS=1 cargo test -p m4tv-shell typescript` rewrites them.
    #[test]
    fn the_typescript_types_are_generated_from_the_rust_ones() {
        let cfg = Config::new();
        let mut generated = String::from(
            "// Generated from the Rust types in crates/shell/src/remote. Do not edit: run\n\
             // `UPDATE_TS=1 cargo test -p m4tv-shell typescript`.\n\n",
        );
        for decl in [
            AppSummary::decl(&cfg),
            LiveState::decl(&cfg),
            PairRequest::decl(&cfg),
            ProveRequest::decl(&cfg),
            KeyRequest::decl(&cfg),
            LaunchRequest::decl(&cfg),
            OrderRequest::decl(&cfg),
            TextRequest::decl(&cfg),
        ] {
            generated += &format!("export {decl}\n\n");
        }
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/remote/src/protocol/api.ts");
        if env::var_os("UPDATE_TS").is_some() {
            fs::write(&path, &generated).unwrap();
        }
        let committed = fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert_eq!(committed, generated, "packages/remote/src/protocol/api.ts is stale");
    }
}

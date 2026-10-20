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
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{mpsc::UnboundedSender, watch};

use super::{Command, LiveState, PairError, Pairings};
use crate::shell::Key;

/// How often an open WebSocket checks that its phone is still paired and still there.
const RECHECK: Duration = Duration::from_secs(1);
/// A quiet phone is pinged after this long, and dropped after `DEAD_AFTER`: one that slept or
/// left the Wi-Fi never closes its socket, and would hide the Pairing Overlay for good.
const PING_AFTER: Duration = Duration::from_secs(5);
const DEAD_AFTER: Duration = Duration::from_secs(15);

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
        .route("/api/ws", get(ws))
        .nest("/api", api)
        .with_state(shared)
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

#[derive(Deserialize)]
struct PairRequest {
    pin: String,
    #[serde(default)]
    name: String,
}

async fn pair(State(shared): State<Shared>, Json(request): Json<PairRequest>) -> Response {
    let name = request.name.trim().chars().take(40).collect::<String>();
    let name = if name.is_empty() { "Phone" } else { &name };
    match shared.pairings.redeem(&request.pin, name) {
        Ok(token) => Json(json!({ "token": token })).into_response(),
        Err(PairError::WrongPin) => refuse(StatusCode::FORBIDDEN, "wrong PIN"),
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

#[derive(Deserialize)]
struct KeyRequest {
    code: String,
}

async fn key(State(shared): State<Shared>, Json(request): Json<KeyRequest>) -> Response {
    match Key::from_code(&request.code) {
        Some(key) => send(&shared, Command::Key(key)),
        None => refuse(StatusCode::BAD_REQUEST, "unknown key code"),
    }
}

#[derive(Deserialize)]
struct LaunchRequest {
    app: String,
}

async fn launch(State(shared): State<Shared>, Json(request): Json<LaunchRequest>) -> Response {
    if !shared.state.borrow().apps.iter().any(|app| app.id == request.app) {
        return refuse(StatusCode::NOT_FOUND, "no such App");
    }
    send(&shared, Command::Launch(request.app))
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
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

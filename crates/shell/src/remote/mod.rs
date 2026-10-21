//! The Remote server: the HTTPS and WebSocket API a phone's Remote uses to control the Shell
//! (ADR 0002). It runs on its own threads inside the Shell's process. The Shell never shares
//! its state with them: the server sends `Command`s over a channel, the Shell publishes a
//! `LiveState` after every change, and `Pairings` is the one thing both sides hold.

mod identity;
pub mod pairing;
mod server;

use std::{
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::Path,
    sync::{Arc, atomic::AtomicUsize, atomic::Ordering},
};

use axum_server::tls_rustls::RustlsConfig;
use m4tv_settings::Value;
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use ts_rs::TS;

pub use pairing::{PairError, Pairings, Phone};

use crate::shell::Key;

/// The name phones are told to find the TV by.
const HOST: &str = "m4tv.local";

/// What the Shell shows, as the Remote sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
pub struct LiveState {
    /// "home", "app" or "settings".
    pub screen: &'static str,
    /// The open App, if there is one.
    pub app: Option<String>,
    /// The focused tile on Home, or row on Settings.
    pub focus: usize,
    /// The Apps in Home's order.
    pub apps: Vec<AppSummary>,
    #[ts(type = "number")]
    pub volume: i64,
    pub muted: bool,
    /// Whether the open App has an editable field focused.
    pub editable: bool,
    /// Every declared setting, in the declaration's order: the Remote builds its screens from this.
    pub settings: Vec<SettingState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SettingState {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: SettingKind,
    #[ts(type = "boolean | number")]
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SettingKind {
    Range {
        #[ts(type = "number")]
        min: i64,
        #[ts(type = "number")]
        max: i64,
        #[ts(type = "number")]
        step: i64,
    },
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct AppSummary {
    pub id: String,
    pub name: String,
}

/// What a phone asks of the Shell. The Shell does these on its own thread.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Key(Key),
    Launch(String),
    Order(Vec<String>),
    Text(String),
    Setting(String, Value),
}

/// The running Remote server, held by the Shell. Dropping it stops the server.
pub struct Remote {
    runtime: Option<tokio::runtime::Runtime>,
    port: u16,
    fingerprint: String,
    cert_replaced: bool,
    pairings: Pairings,
    commands: mpsc::UnboundedReceiver<Command>,
    state: Arc<watch::Sender<LiveState>>,
    connections: Arc<AtomicUsize>,
    mdns: Option<mdns_sd::ServiceDaemon>,
}

impl Remote {
    /// Starts serving on `port` (0 picks a free one), keeping the certificate and paired phones
    /// in `dir`. With `lan`, phones on the network can reach it and the TV announces itself as
    /// `m4tv.local` over mDNS; without, only this machine can (tests, and no firewall prompt).
    pub fn start(dir: &Path, port: u16, lan: bool) -> io::Result<Self> {
        let (identity, created) = identity::load_or_create(dir)?;
        let pairings = Pairings::load(&dir.join("phones.toml"));
        // Phones pinned the old certificate, so none of them can connect any more.
        let cert_replaced = created && pairings.clear()?;

        // The ring provider is process-wide; installing it twice is harmless.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let runtime =
            tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        let bind = if lan { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
        let listener = TcpListener::bind((bind, port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let config =
            runtime.block_on(RustlsConfig::from_pem(identity.cert_pem, identity.key_pem))?;

        let (commands_tx, commands) = mpsc::unbounded_channel();
        let state = Arc::new(watch::channel(LiveState::default()).0);
        let connections = Arc::new(AtomicUsize::new(0));
        let app = server::router(server::Shared {
            pairings: pairings.clone(),
            commands: commands_tx,
            state: state.clone(),
            connections: connections.clone(),
        });
        {
            let _enter = runtime.enter();
            let server = axum_server::from_tcp_rustls(listener, config)?;
            runtime.spawn(async move {
                if let Err(e) = server.serve(app.into_make_service()).await {
                    eprintln!("Remote server stopped: {e}");
                }
            });
        }
        let mdns = lan.then(|| advertise_mdns(port)).flatten();
        Ok(Self {
            runtime: Some(runtime),
            port,
            fingerprint: identity.fingerprint,
            cert_replaced,
            pairings,
            commands,
            state,
            connections,
            mdns,
        })
    }

    /// Where the server listens, as seen from this machine.
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), self.port)
    }

    /// The certificate fingerprint a phone pins.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Whether the certificate was replaced since phones last paired, so none of them can
    /// connect until they pair again.
    pub fn cert_replaced(&self) -> bool {
        self.cert_replaced
    }

    pub fn pairings(&self) -> &Pairings {
        &self.pairings
    }

    /// How many phones have the Remote open right now.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// The link a Pairing QR code carries: the one-time secret and the certificate fingerprint
    /// (for the viewer to compare) ride in the fragment, which a browser never sends to the server.
    pub fn pairing_link(&self, secret: &str) -> String {
        format!("https://{HOST}:{}/#secret={secret}&fp={}", self.port, self.fingerprint)
    }

    pub(crate) fn next_command(&mut self) -> Option<Command> {
        self.commands.try_recv().ok()
    }

    pub(crate) fn publish(&self, live: LiveState) {
        self.state.send_if_modified(|current| {
            let changed = *current != live;
            if changed {
                *current = live;
            }
            changed
        });
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        if let Some(mdns) = &self.mdns {
            let _ = mdns.shutdown();
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

/// Announces `m4tv.local` over mDNS. The Remote works without it (by address), so a network
/// that refuses multicast only costs the name.
fn advertise_mdns(port: u16) -> Option<mdns_sd::ServiceDaemon> {
    let daemon = mdns_sd::ServiceDaemon::new().ok()?;
    let host = format!("{HOST}.");
    let info = mdns_sd::ServiceInfo::new(
        "_m4tv._tcp.local.",
        "m4tv",
        &host,
        "",
        port,
        None::<std::collections::HashMap<String, String>>,
    )
    .ok()?
    .enable_addr_auto();
    daemon.register(info).ok()?;
    Some(daemon)
}

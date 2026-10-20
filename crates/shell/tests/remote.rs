//! Protocol tests: a real HTTPS/WebSocket client against a headless Shell on fakes. This is the
//! only automated seam for the Remote server (#23).

use std::{
    future::Future,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use m4tv_os::FakeOs;
use m4tv_shell::{
    apps::{AppDefinition, built_in_apps},
    engine::{Engine, Frame},
    remote::Remote,
    shell::{Key, Overlay, Shell},
};
use rustls::{
    ClientConfig, DigitallySignedStruct, Error, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{CryptoProvider, ring},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Trusts exactly one certificate, by fingerprint: what a paired phone does.
#[derive(Debug)]
struct Pinned(String);

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let seen: String =
            Sha256::digest(end_entity.as_ref()).iter().map(|b| format!("{b:02x}")).collect();
        if seen == self.0 {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::General("certificate does not match the pinned fingerprint".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

fn pinned_tls(fingerprint: &str) -> ClientConfig {
    let provider: Arc<CryptoProvider> = Arc::new(ring::default_provider());
    ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned(fingerprint.into())))
        .with_no_client_auth()
}

#[derive(Default)]
struct Record {
    events: Vec<String>,
    editable: bool,
}

/// An Engine that records what the Shell asks of it. Clones share one record.
#[derive(Clone, Default)]
struct FakeEngine(Arc<Mutex<Record>>);

impl FakeEngine {
    fn events(&self) -> Vec<String> {
        self.0.lock().unwrap().events.clone()
    }

    fn set_editable(&self, editable: bool) {
        self.0.lock().unwrap().editable = editable;
    }

    fn log(&self, event: String) {
        self.0.lock().unwrap().events.push(event);
    }
}

impl Engine for FakeEngine {
    fn open(&mut self, app: &AppDefinition) {
        self.log(format!("open {}", app.id));
    }
    fn close(&mut self) {
        self.log("close".into());
    }
    fn key(&mut self, key: Key) {
        self.log(format!("key {key:?}"));
    }
    fn can_go_back(&self) -> bool {
        false
    }
    fn go_back(&mut self) {}
    fn take_frame(&mut self) -> Option<Frame> {
        None
    }
    fn editable_focused(&self) -> bool {
        self.0.lock().unwrap().editable
    }
    fn type_text(&mut self, text: &str) {
        self.log(format!("text {text}"));
    }
}

fn temp_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "m4tv-remote-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A headless Shell with its Remote server on a free port, and a client that pins its certificate.
struct Rig {
    dir: PathBuf,
    shell: Shell,
    engine: FakeEngine,
    runtime: tokio::runtime::Runtime,
    addr: SocketAddr,
    fingerprint: String,
    client: reqwest::Client,
    tls: Arc<ClientConfig>,
}

impl Rig {
    fn new() -> Self {
        Self::in_dir(temp_dir())
    }

    /// Boots a Shell in `dir`; booting again in the same dir is a reboot of the same device.
    fn in_dir(dir: PathBuf) -> Self {
        let engine = FakeEngine::default();
        let os = FakeOs::new(dir.join("settings.toml"));
        let mut shell = Shell::new(built_in_apps(), Box::new(os), Box::new(engine.clone()));
        let remote = Remote::start(&dir.join("remote"), 0, false).unwrap();
        let (addr, fingerprint) = (remote.addr(), remote.fingerprint().to_string());
        shell.attach_remote(remote);
        shell.tick(Duration::ZERO);
        let tls = Arc::new(pinned_tls(&fingerprint));
        let client =
            reqwest::Client::builder().use_preconfigured_tls((*tls).clone()).build().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        Self { dir, shell, engine, runtime, addr, fingerprint, client, tls }
    }

    /// Runs `fut` while giving the Shell its turns, as the Dev host's loop does.
    fn drive<T>(&mut self, fut: impl Future<Output = T>) -> T {
        let mut fut = std::pin::pin!(fut);
        let shell = &mut self.shell;
        self.runtime.block_on(async {
            loop {
                tokio::select! {
                    out = &mut fut => return out,
                    _ = tokio::time::sleep(Duration::from_millis(2)) => { shell.tick(Duration::ZERO); }
                }
            }
        })
    }

    fn api(&self) -> Api {
        Api { client: self.client.clone(), addr: self.addr, tls: self.tls.clone() }
    }

    /// The Pairing PIN and QR payload the TV shows.
    fn pairing(&self) -> (String, String) {
        match self.shell.overlay() {
            Some(Overlay::Pairing { pin, payload, .. }) => (pin.clone(), payload.clone()),
            other => panic!("no Pairing Overlay is showing: {other:?}"),
        }
    }

    /// Pairs a phone with the PIN on screen and returns its token.
    fn pair(&mut self, name: &str) -> String {
        let (pin, _) = self.pairing();
        let api = self.api();
        let (status, body) =
            self.drive(api.post("/api/pair", None, json!({"pin": pin, "name": name})));
        assert_eq!(status, 200, "{body}");
        body["token"].as_str().unwrap().to_string()
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Calls the Remote server. Owns what it needs, so it can run while the Shell takes turns.
#[derive(Clone)]
struct Api {
    client: reqwest::Client,
    addr: SocketAddr,
    tls: Arc<ClientConfig>,
}

impl Api {
    fn url(&self, path: &str) -> String {
        format!("https://{}{path}", self.addr)
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self.client.request(method, self.url(path));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        (response.status().as_u16(), response.json().await.unwrap_or(Value::Null))
    }

    async fn post(&self, path: &str, token: Option<&str>, body: Value) -> (u16, Value) {
        self.call(reqwest::Method::POST, path, token, Some(body)).await
    }

    async fn put(&self, path: &str, token: Option<&str>, body: Value) -> (u16, Value) {
        self.call(reqwest::Method::PUT, path, token, Some(body)).await
    }

    async fn get(&self, path: &str, token: Option<&str>) -> (u16, Value) {
        self.call(reqwest::Method::GET, path, token, None).await
    }

    /// Presses a Remote button.
    async fn key(&self, token: &str, code: &str) -> u16 {
        self.post("/api/key", Some(token), json!({ "code": code })).await.0
    }

    /// Opens the live-state WebSocket.
    async fn connect(&self, token: &str) -> Result<Ws, tokio_tungstenite::tungstenite::Error> {
        let url = format!("wss://{}/api/ws?token={token}", self.addr);
        let connector = tokio_tungstenite::Connector::Rustls(self.tls.clone());
        let (ws, _) =
            tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector))
                .await?;
        Ok(ws)
    }
}

/// Reads live states from `ws` until one satisfies `wanted`.
async fn state_where(ws: &mut Ws, wanted: impl Fn(&Value) -> bool) -> Value {
    use futures_util::StreamExt;
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = ws.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = message {
                let state: Value = serde_json::from_str(text.as_str()).unwrap();
                if wanted(&state) {
                    return state;
                }
            }
        }
        panic!("the WebSocket closed before the state arrived");
    })
    .await
    .expect("the state did not arrive in time")
}

/// Waits for the server to close `ws`.
async fn closed(ws: &mut Ws) -> bool {
    use futures_util::StreamExt;
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(message)) = ws.next().await {
            if message.is_close() {
                break;
            }
        }
    })
    .await
    .is_ok()
}

#[test]
fn a_client_pairs_by_pin_and_then_calls_the_api() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let (status, state) = rig.drive(api.get("/api/state", Some(&token)));
    assert_eq!(status, 200);
    assert_eq!(state["screen"], "home");
    assert_eq!(state["volume"], 50);
}

#[test]
fn a_client_pairs_by_the_qr_payload() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (_, payload) = rig.pairing();
    // The payload is a link whose fragment carries the PIN and the certificate fingerprint.
    let fragment = payload.split_once('#').unwrap().1;
    let field = |name: &str| {
        fragment.split('&').find_map(|f| f.strip_prefix(&format!("{name}="))).unwrap().to_string()
    };
    assert_eq!(field("fp"), rig.fingerprint);
    let (status, body) =
        rig.drive(api.post("/api/pair", None, json!({"pin": field("pin"), "name": "QR"})));
    assert_eq!((status, body["token"].is_string()), (200, true));
}

#[test]
fn an_unpaired_client_is_refused() {
    let mut rig = Rig::new();
    let api = rig.api();
    let no_token = rig.drive(api.get("/api/state", None)).0;
    let bad_token = rig.drive(api.get("/api/state", Some("not-a-token"))).0;
    assert_eq!((no_token, bad_token), (401, 401));
}

#[test]
fn a_wrong_pin_is_refused_and_too_many_replace_it_and_pause_pairing() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (pin, _) = rig.pairing();
    let wrong = if pin == "000000" { "111111" } else { "000000" };
    for _ in 0..5 {
        let (status, _) =
            rig.drive(api.post("/api/pair", None, json!({"pin": wrong, "name": "x"})));
        assert_eq!(status, 403);
    }
    rig.shell.tick(Duration::ZERO);
    let (new_pin, _) = rig.pairing();
    assert_ne!(new_pin, pin);
    // The PIN that was on screen no longer works, and neither does the new one for a while.
    let (status, _) = rig.drive(api.post("/api/pair", None, json!({"pin": pin, "name": "x"})));
    assert_eq!(status, 403);
    let (status, _) = rig.drive(api.post("/api/pair", None, json!({"pin": new_pin, "name": "x"})));
    assert_eq!(status, 403, "pairing is paused after too many wrong PINs");
}

#[test]
fn a_client_that_does_not_know_the_fingerprint_cannot_connect() {
    let mut rig = Rig::new();
    let stranger = reqwest::Client::builder()
        .use_preconfigured_tls(pinned_tls(&"0".repeat(64)))
        .build()
        .unwrap();
    let url = rig.api().url("/api/state");
    let result = rig.drive(async { stranger.get(url).send().await });
    assert!(result.is_err());
}

/// Moves the TV's focus from Home to the Settings row at `row`.
fn open_settings_row(rig: &mut Rig, row: usize) {
    for _ in 0..built_in_apps().len() {
        rig.shell.press(Key::Right);
    }
    rig.shell.press(Key::Enter);
    for _ in 0..row {
        rig.shell.press(Key::Down);
    }
}

#[test]
fn revoking_a_phone_in_settings_refuses_its_next_request() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).0, 200);

    // Settings lists the phone after the settings and the Pair row.
    let rows = rig.shell.settings_rows();
    let phone = rows.iter().position(|r| r.label() == "Pixel").expect("the phone is listed");
    open_settings_row(&mut rig, phone);
    rig.shell.press(Key::Enter);

    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).0, 401);
    assert!(rig.shell.settings_rows().iter().all(|r| r.label() != "Pixel"));
}

#[test]
fn a_reboot_keeps_phones_paired_but_a_new_certificate_unpairs_them() {
    let mut rig = Rig::new();
    let token = rig.pair("Pixel");
    let (dir, old_fingerprint) = (rig.dir.clone(), rig.fingerprint.clone());
    drop(rig);

    let mut rig = Rig::in_dir(dir.clone());
    let api = rig.api();
    assert_eq!(rig.fingerprint, old_fingerprint);
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).0, 200);
    drop(rig);

    std::fs::remove_file(dir.join("remote").join("cert.pem")).unwrap();
    let mut rig = Rig::in_dir(dir);
    let api = rig.api();
    assert_ne!(rig.fingerprint, old_fingerprint, "a pin on the old certificate no longer fits");
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).0, 401);
    // The TV says why its phones must pair again.
    assert!(matches!(rig.shell.overlay(), Some(Overlay::Pairing { cert_replaced: true, .. })));
}

#[test]
fn the_remote_buttons_reach_the_open_app_and_the_shell() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    rig.shell.press(Key::Enter); // opens the first App
    assert_eq!(rig.engine.events(), ["open youtube"]);

    let codes = ["ArrowDown", "Enter", "MediaPlayPause"];
    for code in codes {
        assert_eq!(rig.drive(api.key(&token, code)), 204);
    }
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.engine.events()[1..], ["key Down", "key Enter", "key PlayPause"],);

    // Volume belongs to the Shell, and the live state follows it.
    assert_eq!(rig.drive(api.key(&token, "AudioVolumeUp")), 204);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.engine.events().len(), 4, "the App never sees the volume key");
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).1["volume"], 55);

    assert_eq!(rig.drive(api.key(&token, "NotAKey")), 400);
    assert_eq!(rig.drive(api.key("not-a-token", "Enter")), 401);
}

#[test]
fn the_websocket_pushes_app_focus_and_volume_changes() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let mut ws = rig.drive(api.connect(&token)).unwrap();

    let first = rig.drive(state_where(&mut ws, |_| true));
    assert_eq!((first["screen"].as_str(), first["app"].clone()), (Some("home"), Value::Null));

    assert_eq!(rig.drive(api.key(&token, "ArrowRight")), 204);
    let moved = rig.drive(state_where(&mut ws, |s| s["focus"] == 1));
    assert_eq!(moved["screen"], "home");

    assert_eq!(rig.drive(api.key(&token, "AudioVolumeDown")), 204);
    rig.drive(state_where(&mut ws, |s| s["volume"] == 45));

    let (status, _) = rig.drive(api.post("/api/launch", Some(&token), json!({"app": "youtube"})));
    assert_eq!(status, 204);
    let open = rig.drive(state_where(&mut ws, |s| s["app"] == "youtube"));
    assert_eq!(open["screen"], "app");
}

#[test]
fn the_websocket_refuses_a_stranger_and_drops_a_revoked_phone() {
    let mut rig = Rig::new();
    let api = rig.api();
    assert!(rig.drive(api.connect("not-a-token")).is_err());

    let token = rig.pair("Pixel");
    let mut ws = rig.drive(api.connect(&token)).unwrap();
    rig.drive(state_where(&mut ws, |_| true));
    let phone = rig.shell.settings_rows().iter().position(|r| r.label() == "Pixel").unwrap();
    open_settings_row(&mut rig, phone);
    rig.shell.press(Key::Enter);
    assert!(rig.drive(closed(&mut ws)));
}

#[test]
fn the_pairing_overlay_stays_until_a_phone_has_the_remote_open() {
    let mut rig = Rig::new();
    let api = rig.api();
    // Nobody has the Remote open: the Overlay is up and does not time out.
    rig.shell.tick(Duration::from_secs(3600));
    assert!(matches!(rig.shell.overlay(), Some(Overlay::Pairing { .. })));
    // A volume change does not cover it.
    rig.shell.press(Key::VolumeUp);
    assert!(matches!(rig.shell.overlay(), Some(Overlay::Pairing { .. })));

    let token = rig.pair("Pixel");
    let ws = rig.drive(api.connect(&token)).unwrap();
    rig.drive(async { tokio::time::sleep(Duration::from_millis(50)).await });
    assert_eq!(rig.shell.overlay(), None, "a phone has the Remote open");

    // It does not cover a playing App, even with no phone connected.
    drop(ws);
    rig.drive(async { tokio::time::sleep(Duration::from_millis(200)).await });
    assert!(matches!(rig.shell.overlay(), Some(Overlay::Pairing { .. })));
    rig.shell.press(Key::Enter);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.shell.overlay(), None);
    rig.shell.press(Key::Home);
    rig.shell.tick(Duration::ZERO);
    assert!(matches!(rig.shell.overlay(), Some(Overlay::Pairing { .. })));
}

#[test]
fn settings_can_bring_the_pairing_overlay_back_for_another_phone() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let _ws = rig.drive(api.connect(&token)).unwrap();
    rig.drive(async { tokio::time::sleep(Duration::from_millis(50)).await });
    assert_eq!(rig.shell.overlay(), None);

    let pair_row = rig.shell.settings_rows().iter().position(|r| r.label() == "Pair a phone");
    open_settings_row(&mut rig, pair_row.expect("Settings has a Pair row"));
    rig.shell.press(Key::Enter);
    assert!(matches!(rig.shell.overlay(), Some(Overlay::Pairing { .. })));
    rig.shell.press(Key::Back);
    assert_eq!(rig.shell.overlay(), None);

    // A second phone can pair with the PIN once it is shown again.
    rig.shell.press(Key::Enter);
    let second = rig.pair("Tablet");
    assert_ne!(second, token);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.shell.overlay(), None, "pairing closes the Overlay");
}

#[test]
fn a_phone_launches_and_reorders_apps() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");

    let (status, _) = rig.drive(api.post("/api/launch", Some(&token), json!({"app": "nope"})));
    assert_eq!(status, 404);
    rig.drive(api.post("/api/launch", Some(&token), json!({"app": "youtube"})));
    rig.drive(api.post("/api/launch", Some(&token), json!({"app": "spotify"})));
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.engine.events(), ["open youtube", "close", "open spotify"]);

    rig.shell.press(Key::Home);
    rig.shell.press(Key::Right); // focus Spotify
    let order = json!({"apps": ["netflix", "spotify", "youtube"]});
    assert_eq!(rig.drive(api.put("/api/apps/order", Some(&token), order)).0, 204);
    rig.shell.tick(Duration::ZERO);
    let ids: Vec<_> = rig.shell.apps().iter().map(|app| app.id.as_str()).collect();
    assert_eq!(ids, ["netflix", "spotify", "youtube"]);
    assert_eq!(rig.shell.focused(), 1, "the focus stays on Spotify");
    let state = rig.drive(api.get("/api/state", Some(&token))).1;
    assert_eq!(state["apps"][0]["id"], "netflix");

    for bad in [json!({"apps": ["netflix"]}), json!({"apps": ["netflix", "netflix", "youtube"]})] {
        assert_eq!(rig.drive(api.put("/api/apps/order", Some(&token), bad)).0, 400);
    }
}

#[test]
fn text_is_committed_into_the_focused_field_or_refused() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let mut ws = rig.drive(api.connect(&token)).unwrap();
    let say = json!({"text": "cat videos"});

    // Nothing is open, so there is no field.
    let (status, body) = rig.drive(api.post("/api/text", Some(&token), say.clone()));
    assert_eq!((status, body["error"].as_str()), (409, Some("no editable field is focused")));

    rig.shell.press(Key::Enter); // opens an App
    let (status, _) = rig.drive(api.post("/api/text", Some(&token), say.clone()));
    assert_eq!(status, 409, "the App has no editable field focused");

    // The Shell reports a field gaining the focus, and losing it.
    rig.engine.set_editable(true);
    rig.shell.tick(Duration::ZERO);
    rig.drive(state_where(&mut ws, |s| s["editable"] == true));
    let (status, _) = rig.drive(api.post("/api/text", Some(&token), say));
    assert_eq!(status, 204);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.engine.events().last().unwrap(), "text cat videos");

    rig.engine.set_editable(false);
    rig.shell.tick(Duration::ZERO);
    rig.drive(state_where(&mut ws, |s| s["editable"] == false));
}

#[test]
#[ignore = "needs multicast on the local network"]
fn the_tv_announces_itself_over_mdns() {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    let remote = Remote::start(&temp_dir(), 0, true).unwrap();
    let port = remote.addr().port();
    let browser = ServiceDaemon::new().unwrap();
    let events = browser.browse("_m4tv._tcp.local.").unwrap();
    let found = std::iter::from_fn(|| events.recv_timeout(Duration::from_secs(10)).ok()).find_map(
        |event| match event {
            ServiceEvent::ServiceResolved(info) => Some(info),
            _ => None,
        },
    );
    let info = found.expect("the TV was found by mDNS");
    assert_eq!((info.get_hostname(), info.get_port()), ("m4tv.local.", port));
}

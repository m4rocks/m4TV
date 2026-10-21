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

use m4tv_os::{FakeOs, Network, Security};
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
    os: FakeOs,
    /// Where the plain-HTTP Remote listens while the Setup hotspot is up.
    hotspot_addr: SocketAddr,
}

impl Rig {
    fn new() -> Self {
        Self::in_dir(temp_dir())
    }

    /// Boots a Shell in `dir`; booting again in the same dir is a reboot of the same device.
    fn in_dir(dir: PathBuf) -> Self {
        Self::in_dir_with(dir, |_| {})
    }

    /// A device with no network, in a room with these Wi-Fis: "Home" (WPA2, hunter22), "Cafe"
    /// (open), and "Secret" (WPA3, hunter22, hidden).
    fn offline() -> Self {
        Self::in_dir_with(temp_dir(), |os| {
            os.set_network(Network::None);
            os.add_wifi("Home", Security::Wpa2, Some("hunter22"), false, "10.0.0.7");
            os.add_wifi("Cafe", Security::Open, None, false, "10.0.0.8");
            os.add_wifi("Secret", Security::Wpa3, Some("hunter22"), true, "10.0.0.9");
        })
    }

    /// Boots with the device set up by `setup` first.
    fn in_dir_with(dir: PathBuf, setup: impl FnOnce(&FakeOs)) -> Self {
        let engine = FakeEngine::default();
        let os = FakeOs::new(dir.join("settings.toml"));
        setup(&os);
        let mut shell = Shell::new(built_in_apps(), Box::new(os.clone()), Box::new(engine.clone()));
        let hotspot_port =
            std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let remote =
            Remote::start(&dir.join("remote"), 0, false).unwrap().with_hotspot_port(hotspot_port);
        let (addr, fingerprint) = (remote.addr(), remote.fingerprint().to_string());
        shell.attach_remote(remote);
        shell.tick(Duration::ZERO);
        let tls = Arc::new(pinned_tls(&fingerprint));
        let client =
            reqwest::Client::builder().use_preconfigured_tls((*tls).clone()).build().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let hotspot_addr = SocketAddr::from(([127, 0, 0, 1], hotspot_port));
        Self { dir, shell, engine, runtime, addr, fingerprint, client, tls, os, hotspot_addr }
    }

    /// The Remote as a phone on the Setup hotspot sees it: plain HTTP.
    fn hotspot_api(&self) -> Api {
        Api {
            client: reqwest::Client::new(),
            addr: self.hotspot_addr,
            tls: self.tls.clone(),
            scheme: "http",
        }
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
        Api { client: self.client.clone(), addr: self.addr, tls: self.tls.clone(), scheme: "https" }
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
    scheme: &'static str,
}

impl Api {
    fn url(&self, path: &str) -> String {
        format!("{}://{}{path}", self.scheme, self.addr)
    }

    /// Whether anything answers at `path`: the hotspot's listener is gone once it drops.
    async fn reachable(&self, path: &str) -> bool {
        self.client.get(self.url(path)).send().await.is_ok()
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

    /// Fetches a page: status, content type and body.
    async fn page(&self, path: &str) -> (u16, String, String) {
        let response = self.client.get(self.url(path)).send().await.unwrap();
        let status = response.status().as_u16();
        let kind = response
            .headers()
            .get("content-type")
            .map_or(String::new(), |v| v.to_str().unwrap().to_string());
        (status, kind, response.text().await.unwrap())
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

/// A field of the QR link's fragment.
fn fragment_field(payload: &str, name: &str) -> String {
    let fragment = payload.split_once('#').unwrap().1;
    fragment.split('&').find_map(|f| f.strip_prefix(&format!("{name}="))).unwrap().to_string()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn random_hex() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the Remote page computes with `@noble/hashes`: HMAC-SHA256 over `role:challenge:nonce`,
/// keyed by the QR secret.
fn mac(secret: &str, role: &str, challenge: &str, nonce: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = Hmac::<Sha256>::new_from_slice(&unhex(secret)).unwrap();
    mac.update(format!("{role}:{challenge}:{nonce}").as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Pairs the way the Remote page does after a scan: asks for a challenge, proves it knows the
/// QR `secret`, checks the TV proves it too, and works out its token (which never travels).
/// `post` is the wire to the TV, so a test can put something in the middle.
async fn pair_by_secret(
    post: impl AsyncFn(&str, Value) -> (u16, Value),
    secret: &str,
    name: &str,
) -> Result<String, String> {
    let (status, body) = post("/api/pair/challenge", json!({})).await;
    if status != 200 {
        return Err(format!("the TV refused a challenge: {status}"));
    }
    let challenge = body["challenge"].as_str().unwrap().to_string();
    let nonce = random_hex();
    let request = json!({
        "challenge": challenge,
        "nonce": nonce,
        "proof": mac(secret, "phone", &challenge, &nonce),
        "name": name,
    });
    let (status, body) = post("/api/pair/prove", request).await;
    if status != 200 {
        return Err(format!("the TV refused the proof: {status}"));
    }
    if body["proof"] != mac(secret, "tv", &challenge, &nonce) {
        return Err("the TV did not prove it knows the secret".into());
    }
    Ok(mac(secret, "token", &challenge, &nonce))
}

#[test]
fn a_client_pairs_by_the_qr_secret_and_not_the_pin() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (pin, payload) = rig.pairing();
    assert!(!payload.contains(&pin), "the QR link must not carry the PIN");
    assert_eq!(fragment_field(&payload, "fp"), rig.fingerprint);

    let secret = fragment_field(&payload, "secret");
    let token = rig
        .drive(pair_by_secret(async |path, body| api.post(path, None, body).await, &secret, "QR"))
        .unwrap();
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).0, 200);
    assert!(rig.shell.settings_rows().iter().any(|r| r.label() == "QR"));
}

#[test]
fn a_wrong_secret_is_refused_and_too_many_replace_the_secret_and_pause_pairing() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (_, payload) = rig.pairing();
    let secret = fragment_field(&payload, "secret");
    let wrong = "00".repeat(16);
    for _ in 0..5 {
        let result = rig.drive(pair_by_secret(
            async |path, body| api.post(path, None, body).await,
            &wrong,
            "x",
        ));
        assert!(result.is_err());
    }
    rig.shell.tick(Duration::ZERO);
    let (_, new_payload) = rig.pairing();
    assert_ne!(fragment_field(&new_payload, "secret"), secret);
    // The secret that was on screen is dead, and pairing is paused for the new one too.
    for secret in [secret, fragment_field(&new_payload, "secret")] {
        let result = rig.drive(pair_by_secret(
            async |path, body| api.post(path, None, body).await,
            &secret,
            "x",
        ));
        assert!(result.is_err(), "pairing is paused after too many wrong tries");
    }
}

#[test]
fn wrong_pins_also_kill_the_secret() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (pin, payload) = rig.pairing();
    let secret = fragment_field(&payload, "secret");
    let wrong = if pin == "000000" { "111111" } else { "000000" };
    for _ in 0..5 {
        rig.drive(api.post("/api/pair", None, json!({"pin": wrong, "name": "x"})));
    }
    let result = rig.drive(pair_by_secret(
        async |path, body| api.post(path, None, body).await,
        &secret,
        "x",
    ));
    assert!(result.is_err());
}

#[test]
fn a_replayed_proof_or_a_reused_secret_is_refused() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (_, payload) = rig.pairing();
    let secret = fragment_field(&payload, "secret");

    // Record the phone's messages while it pairs.
    let sent = Mutex::new(Vec::new());
    let token = rig
        .drive(pair_by_secret(
            async |path, body| {
                sent.lock().unwrap().push((path.to_string(), body.clone()));
                api.post(path, None, body).await
            },
            &secret,
            "Pixel",
        ))
        .unwrap();
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).0, 200);

    for (path, body) in sent.into_inner().unwrap() {
        if path.ends_with("/prove") {
            let replay = rig.drive(api.post(&path, None, body)).0;
            assert_eq!(replay, 403, "a proof works once");
        }
    }
    // The secret died with the pairing, even for a client that knows it.
    let again = rig.drive(pair_by_secret(
        async |path, body| api.post(path, None, body).await,
        &secret,
        "Again",
    ));
    assert!(again.is_err());
}

#[test]
fn proofs_for_challenges_the_tv_never_gave_do_not_burn_the_tries() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (_, payload) = rig.pairing();
    let secret = fragment_field(&payload, "secret");
    for _ in 0..10 {
        let bogus = json!({"challenge": random_hex(), "nonce": "00", "proof": random_hex()});
        assert_eq!(rig.drive(api.post("/api/pair/prove", None, bogus)).0, 403);
    }
    // Nobody was locked out and the secret is the one on screen.
    let paired = rig.drive(pair_by_secret(
        async |path, body| api.post(path, None, body).await,
        &secret,
        "Pixel",
    ));
    assert!(paired.is_ok());
}

#[test]
fn a_client_catches_a_tv_that_does_not_know_the_secret() {
    let mut rig = Rig::new();
    let (_, payload) = rig.pairing();
    let secret = fragment_field(&payload, "secret");
    // An impostor answers the challenge and accepts any proof, but cannot make the TV's proof.
    let result = rig.drive(pair_by_secret(
        async |path, _| {
            if path.ends_with("/challenge") {
                (200, json!({ "challenge": random_hex() }))
            } else {
                (200, json!({ "proof": random_hex() }))
            }
        },
        &secret,
        "x",
    ));
    assert_eq!(result.unwrap_err(), "the TV did not prove it knows the secret");
}

#[test]
fn a_proxy_that_relays_the_pairing_without_the_secret_cannot_use_it() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (pin, payload) = rig.pairing();
    let secret = fragment_field(&payload, "secret");

    // The phone pairs through a proxy that sees every message in both directions.
    let seen = Mutex::new(Vec::new());
    let token = rig
        .drive(pair_by_secret(
            async |path, body| {
                let (status, reply) = api.post(path, None, body.clone()).await;
                seen.lock().unwrap().extend([body, reply.clone()]);
                (status, reply)
            },
            &secret,
            "Pixel",
        ))
        .unwrap();

    // Nothing it saw is a token: the token is worked out from the secret and never sent.
    let strings: Vec<String> = seen
        .into_inner()
        .unwrap()
        .iter()
        .flat_map(|message| message.as_object().unwrap().values().cloned().collect::<Vec<_>>())
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    assert!(!strings.contains(&token));
    for guess in strings {
        assert_eq!(rig.drive(api.get("/api/state", Some(&guess))).0, 401);
    }
    // And the PIN that was on screen pairs nothing either.
    let late = rig.drive(api.post("/api/pair", None, json!({"pin": pin, "name": "Proxy"})));
    assert_eq!(late.0, 403);
}

#[test]
fn the_remote_web_app_is_served_with_a_landing_page_that_needs_no_javascript() {
    let mut rig = Rig::new();
    let api = rig.api();
    let (status, kind, page) = rig.drive(api.page("/"));
    assert_eq!(status, 200);
    assert!(kind.starts_with("text/html"), "{kind}");
    // The page itself says where to open, without a script: the address it was fetched from.
    assert!(page.contains(&format!("https://{}/", rig.addr)), "{page}");

    // Every file the page loads is served, with the type a browser needs to run it.
    let files: Vec<&str> =
        page.split(['"', '\'']).filter(|part| part.starts_with("/assets/")).collect();
    for file in files {
        let (status, kind, _) = rig.drive(api.page(file));
        let wanted = if file.ends_with(".js") { "text/javascript" } else { "text/css" };
        assert_eq!((status, kind.as_str()), (200, wanted), "{file}");
    }
    assert_eq!(rig.drive(api.page("/nothing-here")).0, 404);
    let post = rig.drive(api.post("/", None, json!({}))).0;
    assert_eq!(post, 405, "the web app is read-only");
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

#[test]
fn the_remote_lists_every_declared_setting_and_changes_converge_both_ways() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let mut ws = rig.drive(api.connect(&token)).unwrap();

    // Read: one entry per declaration, in its order, with the same options as the Shell.
    let first = rig.drive(state_where(&mut ws, |_| true));
    let listed: Vec<(&str, &str)> = first["settings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["key"].as_str().unwrap(), s["label"].as_str().unwrap()))
        .collect();
    let declared: Vec<(&str, &str)> =
        m4tv_settings::SETTINGS.iter().map(|d| (d.key, d.label)).collect();
    assert_eq!(listed, declared);
    let volume = &first["settings"][0];
    assert_eq!(volume["kind"], json!({"type": "range", "min": 0, "max": 100, "step": 5}));
    assert_eq!(volume["value"], 50);
    assert_eq!(first["settings"][1]["kind"], json!({"type": "toggle"}));
    assert_eq!(first["settings"][1]["value"], false);

    // Write from the phone: the Shell takes it and the push follows.
    let (status, _) =
        rig.drive(api.put("/api/settings/volume", Some(&token), json!({"value": 70})));
    assert_eq!(status, 204);
    rig.drive(state_where(&mut ws, |s| s["settings"][0]["value"] == 70 && s["volume"] == 70));
    let (status, _) =
        rig.drive(api.put("/api/settings/mute", Some(&token), json!({"value": true})));
    assert_eq!(status, 204);
    rig.drive(state_where(&mut ws, |s| s["settings"][1]["value"] == true && s["muted"] == true));

    // Write from the Shell: the phone sees it without asking.
    rig.shell.press(Key::VolumeUp);
    rig.drive(state_where(&mut ws, |s| s["settings"][0]["value"] == 75));
}

#[test]
fn the_remote_refuses_settings_the_shell_would_not_take() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let put = |key: &str, body: Value| {
        let (api, token, path) = (&api, &token, format!("/api/settings/{key}"));
        async move { api.put(&path, Some(token), body).await }
    };

    assert_eq!(rig.drive(put("nope", json!({"value": 1}))).0, 404);
    assert_eq!(rig.drive(put("volume", json!({"value": 101}))).0, 400);
    assert_eq!(rig.drive(put("volume", json!({"value": 52}))).0, 400, "off the step");
    assert_eq!(rig.drive(put("volume", json!({"value": true}))).0, 400);
    assert_eq!(rig.drive(put("mute", json!({"value": 1}))).0, 400);
    assert_eq!(rig.drive(api.put("/api/settings/volume", None, json!({"value": 60}))).0, 401);
    assert_eq!(rig.drive(api.get("/api/state", Some(&token))).1["volume"], 50);
}

// The Setup hotspot flow (#29), on the fake network.

/// The hotspot Overlay the TV shows: name, password, Wi-Fi QR payload and status line.
fn hotspot_overlay(rig: &Rig) -> (String, String, String, String) {
    match rig.shell.overlay() {
        Some(Overlay::Hotspot { ssid, password, payload, status, .. }) => {
            (ssid.clone(), password.clone(), payload.clone(), status.clone())
        }
        other => panic!("no hotspot Overlay is showing: {other:?}"),
    }
}

/// Pairs the way a phone on the Setup hotspot does: no PIN, because joining the hotspot with its
/// password is the proof.
fn pair_on_hotspot(rig: &mut Rig) -> String {
    let http = rig.hotspot_api();
    let (status, body) = rig.drive(http.post("/api/pair/hotspot", None, json!({"name": "Pixel"})));
    assert_eq!(status, 200, "{body}");
    body["token"].as_str().unwrap().to_string()
}

/// The `setup` part of the live state, as a phone on the hotspot reads it.
fn setup_state(rig: &mut Rig, token: &str) -> Value {
    let http = rig.hotspot_api();
    let (status, state) = rig.drive(http.get("/api/state", Some(token)));
    assert_eq!(status, 200);
    state["setup"].clone()
}

fn join_status(rig: &mut Rig, token: &str, body: Value) -> u16 {
    let http = rig.hotspot_api();
    rig.drive(http.post("/api/wifi/join", Some(token), body)).0
}

#[test]
fn with_no_network_the_tv_shows_the_hotspot_as_a_wifi_qr_and_ethernet_gets_none() {
    let rig = Rig::offline();
    let (ssid, password, payload, _) = hotspot_overlay(&rig);
    assert_eq!(
        rig.os.hotspot().map(|h| (h.ssid, h.password)),
        Some((ssid.clone(), password.clone()))
    );
    assert_eq!(payload, format!("WIFI:T:WPA;S:{ssid};P:{password};;"));
    assert!(password.len() >= 8 && password.chars().all(|c| c.is_ascii_alphanumeric()));

    // On Ethernet there is no hotspot, only the ordinary Pairing Overlay.
    let rig = Rig::in_dir_with(temp_dir(), |os| os.set_network(Network::Ethernet));
    assert_eq!(rig.os.hotspot(), None);
    rig.pairing();

    // A cable plugged in later takes the hotspot down.
    let mut rig = Rig::offline();
    rig.os.set_network(Network::Ethernet);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.os.hotspot(), None);
    rig.pairing();
}

#[test]
fn the_remote_is_served_over_plain_http_while_the_hotspot_is_up_and_hands_over_the_fingerprint() {
    let mut rig = Rig::offline();
    let http = rig.hotspot_api();
    let (status, body) = rig.drive(http.get("/api/hotspot", None));
    assert_eq!(status, 200);
    assert_eq!(body["fingerprint"], rig.fingerprint.as_str());
    assert_eq!(rig.drive(http.get("/api/state", None)).0, 401, "the API still needs a phone");

    // The TV tells the viewer where to point the browser: the address the OS gave the hotspot.
    let Some(Overlay::Hotspot { url, .. }) = rig.shell.overlay() else { panic!("no hotspot") };
    assert_eq!(*url, format!("http://{}", rig.hotspot_addr));

    // The PIN and QR-secret pairing are the LAN's: this PIN-less listener must not offer them.
    for path in ["/api/pair", "/api/pair/challenge", "/api/pair/prove"] {
        assert_eq!(rig.drive(http.post(path, None, json!({}))).0, 405, "{path}");
    }

    // The open pairing exists only on the hotspot's listener, never on the LAN's HTTPS.
    let api = rig.api();
    // (A POST there reaches the static-file fallback, which only answers GET.)
    assert_eq!(rig.drive(api.post("/api/pair/hotspot", None, json!({"name": "X"}))).0, 405);
    assert_eq!(rig.drive(api.get("/api/hotspot", None)).0, 404);

    // Once the TV has a network the listener is gone.
    rig.os.set_network(Network::Ethernet);
    rig.shell.tick(Duration::ZERO);
    assert!(!rig.drive(http.reachable("/api/hotspot")));
}

#[test]
fn a_phone_on_the_hotspot_picks_a_wifi_and_the_tv_hands_over_to_its_new_address() {
    let mut rig = Rig::offline();
    let token = pair_on_hotspot(&mut rig);

    let setup = setup_state(&mut rig, &token);
    assert_eq!(setup["phase"], "hotspot");
    assert_eq!(
        setup["networks"],
        json!([{"ssid": "Home", "security": "wpa2"}, {"ssid": "Cafe", "security": "open"}]),
        "hidden networks are not listed"
    );

    let join = json!({"ssid": "Home", "password": "hunter22", "security": "wpa2", "hidden": false});
    assert_eq!(join_status(&mut rig, &token, join), 204);
    rig.shell.tick(Duration::ZERO);

    // The TV shows the new address with a Pairing QR for it, and the phone is told too.
    let Some(Overlay::Joined { address, payload }) = rig.shell.overlay().cloned() else {
        panic!("no handover Overlay: {:?}", rig.shell.overlay());
    };
    assert_eq!(address, "10.0.0.7");
    assert!(payload.starts_with("https://10.0.0.7:"), "{payload}");
    assert_eq!(fragment_field(&payload, "fp"), rig.fingerprint);
    assert_eq!(setup_state(&mut rig, &token), json!({"phase": "joined", "address": "10.0.0.7"}));

    // Then the hotspot drops, listener included.
    assert!(rig.os.hotspot().is_some());
    rig.shell.tick(Duration::from_secs(5));
    assert_eq!(rig.os.hotspot(), None);
    let http = rig.hotspot_api();
    assert!(!rig.drive(http.reachable("/api/hotspot")), "the hotspot's listener is gone");

    // Scanning it pairs the phone afresh, and it controls the TV from the LAN.
    let api = rig.api();
    let secret = fragment_field(&payload, "secret");
    let token = rig
        .drive(pair_by_secret(
            async |path, body| api.post(path, None, body).await,
            &secret,
            "Pixel",
        ))
        .unwrap();
    assert_eq!(rig.drive(api.key(&token, "ArrowRight")), 204);
}

#[test]
fn the_join_request_takes_wpa2_wpa3_open_and_hidden_networks() {
    for (ssid, password, security, hidden, address) in [
        ("Home", Some("hunter22"), "wpa2", false, "10.0.0.7"),
        ("Cafe", None, "open", false, "10.0.0.8"),
        ("Secret", Some("hunter22"), "wpa3", true, "10.0.0.9"),
    ] {
        let mut rig = Rig::offline();
        let token = pair_on_hotspot(&mut rig);
        let body =
            json!({"ssid": ssid, "password": password, "security": security, "hidden": hidden});
        assert_eq!(join_status(&mut rig, &token, body), 204, "{ssid}");
        rig.shell.tick(Duration::ZERO);
        assert!(
            matches!(rig.shell.overlay(), Some(Overlay::Joined { address: a, .. }) if a == address)
        );
    }
}

#[test]
fn the_join_request_is_checked_and_needs_a_paired_phone_in_setup() {
    let mut rig = Rig::offline();
    let token = pair_on_hotspot(&mut rig);
    let wpa = |ssid: &str, password: Value| json!({"ssid": ssid, "password": password, "security": "wpa2", "hidden": false});
    for bad in [
        wpa("", json!("hunter22")),
        wpa(&"x".repeat(33), json!("hunter22")),
        wpa("Home", json!("short")),
        wpa("Home", json!("x".repeat(64))),
        wpa("Home", Value::Null),
        json!({"ssid": "Cafe", "password": "hunter22", "security": "open", "hidden": false}),
        json!({"ssid": "Home", "password": "hunter22", "security": "wep", "hidden": false}),
    ] {
        assert_eq!(join_status(&mut rig, &token, bad.clone()), 400, "{bad}");
    }
    assert_eq!(setup_state(&mut rig, &token)["phase"], "hotspot", "nothing started");

    let http = rig.hotspot_api();
    let good = wpa("Home", json!("hunter22"));
    assert_eq!(rig.drive(http.post("/api/wifi/join", None, good.clone())).0, 401);

    // A connected TV is not in setup: a paired phone cannot move it to another network.
    let mut rig = Rig::new();
    let token = rig.pair("Pixel");
    let api = rig.api();
    assert_eq!(rig.drive(api.post("/api/wifi/join", Some(&token), good)).0, 409);
}

#[test]
fn a_failed_join_brings_the_hotspot_back_with_the_reason() {
    let mut rig = Rig::offline();
    let token = pair_on_hotspot(&mut rig);
    let body =
        json!({"ssid": "Home", "password": "wrongpass", "security": "wpa2", "hidden": false});
    assert_eq!(join_status(&mut rig, &token, body), 204);
    rig.shell.tick(Duration::ZERO);

    let setup = setup_state(&mut rig, &token);
    assert_eq!(
        (setup["phase"].as_str(), setup["reason"].as_str()),
        (Some("hotspot"), Some("wrong password"))
    );
    assert!(rig.os.hotspot().is_some());
    assert_eq!(hotspot_overlay(&rig).3, "wrong password");
}

#[test]
fn no_address_within_thirty_seconds_counts_as_a_failed_join() {
    let mut rig = Rig::offline();
    rig.os.stall_dhcp(true);
    let token = pair_on_hotspot(&mut rig);
    let body = json!({"ssid": "Home", "password": "hunter22", "security": "wpa2", "hidden": false});
    assert_eq!(join_status(&mut rig, &token, body), 204);

    rig.shell.tick(Duration::from_secs(29));
    assert_eq!(setup_state(&mut rig, &token)["phase"], "joining");
    rig.shell.tick(Duration::from_secs(2));
    let setup = setup_state(&mut rig, &token);
    assert_eq!(setup["phase"], "hotspot");
    assert!(setup["reason"].as_str().unwrap().contains("30"), "{setup}");
    assert!(rig.os.hotspot().is_some());
}

#[test]
fn settings_starts_the_hotspot_on_demand_except_on_ethernet() {
    let mut rig = Rig::new();
    let row = rig.shell.settings_rows().iter().position(|r| r.label() == "Setup hotspot").unwrap();
    open_settings_row(&mut rig, row);
    rig.shell.press(Key::Enter);
    assert!(rig.os.hotspot().is_some());
    hotspot_overlay(&rig);

    // Back puts it away, as the network is fine.
    rig.shell.press(Key::Back);
    assert_eq!(rig.os.hotspot(), None);

    let mut rig = Rig::in_dir_with(temp_dir(), |os| os.set_network(Network::Ethernet));
    open_settings_row(&mut rig, row);
    rig.shell.press(Key::Enter);
    assert_eq!(rig.os.hotspot(), None);
    assert!(rig.shell.notice().unwrap().contains("Ethernet"));
}

#[test]
fn a_join_that_was_given_up_on_cannot_finish_later_and_pull_the_tv_off_the_hotspot() {
    let mut rig = Rig::offline();
    rig.os.stall_dhcp(true);
    let token = pair_on_hotspot(&mut rig);
    let body = json!({"ssid": "Home", "password": "hunter22", "security": "wpa2", "hidden": false});
    assert_eq!(join_status(&mut rig, &token, body), 204);
    rig.shell.tick(Duration::from_secs(31));
    assert_eq!(setup_state(&mut rig, &token)["phase"], "hotspot");

    // The router answers at last, too late.
    rig.os.stall_dhcp(false);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(setup_state(&mut rig, &token)["phase"], "hotspot");
    assert!(rig.os.hotspot().is_some());
}

#[test]
fn the_hotspot_comes_back_if_the_network_is_lost_after_a_join() {
    let mut rig = Rig::offline();
    let token = pair_on_hotspot(&mut rig);
    let body = json!({"ssid": "Home", "password": "hunter22", "security": "wpa2", "hidden": false});
    assert_eq!(join_status(&mut rig, &token, body), 204);
    rig.shell.tick(Duration::ZERO); // the join registers
    rig.shell.tick(Duration::from_secs(5)); // and the hotspot's grace runs out
    assert_eq!(rig.os.hotspot(), None);

    rig.os.set_network(Network::None);
    rig.shell.tick(Duration::ZERO); // the handover is over
    rig.shell.tick(Duration::ZERO); // and the hotspot is back
    assert!(rig.os.hotspot().is_some());
    hotspot_overlay(&rig);
    assert_eq!(setup_state(&mut rig, &token)["phase"], "hotspot");
}

#[test]
fn a_volume_change_from_a_phone_shows_on_the_tv_like_one_from_its_keys() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    // Pairing keeps its Overlay on screen until a phone has the Remote open.
    let mut ws = rig.drive(api.connect(&token)).unwrap();
    rig.drive(state_where(&mut ws, |_| true)); // the Remote is counted as open before it is sent
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.shell.overlay(), None);
    let (status, _) =
        rig.drive(api.put("/api/settings/volume", Some(&token), json!({"value": 70})));
    assert_eq!(status, 204);
    rig.shell.tick(Duration::ZERO);
    assert_eq!(rig.shell.overlay(), Some(&Overlay::Volume { level: 70, muted: false }));
}

#[test]
fn only_sixteen_phones_can_pair_on_the_hotspot() {
    let mut rig = Rig::offline();
    for _ in 0..16 {
        pair_on_hotspot(&mut rig);
    }
    let http = rig.hotspot_api();
    let (status, body) = rig.drive(http.post("/api/pair/hotspot", None, json!({"name": "17th"})));
    assert_eq!(status, 409, "{body}");
}

#[test]
fn quick_steps_from_a_phone_all_count_because_the_shell_applies_them_to_its_own_value() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let step = |key: &str, direction: i64| {
        let (api, token, path) = (&api, &token, format!("/api/settings/{key}/step"));
        async move { api.post(&path, Some(token), json!({"direction": direction})).await.0 }
    };

    // Three taps before the phone has heard back from any: none is lost.
    for _ in 0..3 {
        assert_eq!(rig.drive(step("volume", 1)), 204);
    }
    assert_eq!(rig.drive(step("mute", 1)), 204);
    rig.shell.tick(Duration::ZERO);
    let state = rig.drive(api.get("/api/state", Some(&token))).1;
    assert_eq!((state["volume"].as_i64(), state["muted"].as_bool()), (Some(65), Some(true)));

    assert_eq!(rig.drive(step("nope", 1)), 404);
    assert_eq!(rig.drive(step("volume", 0)), 400);
    assert_eq!(rig.drive(step("volume", 5)), 400);
}

#[test]
fn a_setting_the_shell_could_not_save_is_reported_to_the_phone() {
    let mut rig = Rig::new();
    let api = rig.api();
    let token = rig.pair("Pixel");
    let mut ws = rig.drive(api.connect(&token)).unwrap();
    rig.drive(state_where(&mut ws, |s| s["notice"].is_null()));

    // A directory where the settings file belongs: the Shell cannot replace it.
    std::fs::create_dir_all(rig.dir.join("settings.toml")).unwrap();
    let (status, _) =
        rig.drive(api.put("/api/settings/volume", Some(&token), json!({"value": 70})));
    assert_eq!(status, 204);
    rig.drive(state_where(&mut ws, |s| {
        s["notice"].as_str().is_some_and(|n| n.contains("Could not save settings"))
    }));
}

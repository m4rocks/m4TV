//! The Electron Engine's side of the host protocol (ADR 0006), with a test standing in for the host
//! over the real socket. The child it supervises is a small Node script, which every build here
//! already needs for the Remote.

use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use m4tv_shell::{
    apps::built_in_apps,
    electron_engine::{ElectronEngine, EngineConfig},
    engine::Engine,
    shell::Key,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines, WriteHalf};

/// A child that stays alive and says so in a file, so a test can tell if it still runs.
const ALIVE: &str = "const fs = require('fs'); fs.appendFileSync(process.env.MARK, 's'); \
                     setInterval(() => fs.appendFileSync(process.env.BEAT, 'b'), 20);";
/// A child that dies at once.
const DIES: &str = "require('fs').appendFileSync(process.env.MARK, 's')";

struct Rig {
    dir: PathBuf,
    endpoint: String,
    config: Option<EngineConfig>,
}

impl Rig {
    fn new(child: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("m4tv-engine-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = EngineConfig::new(
            "node".into(),
            vec![OsString::from("-e"), child.into()],
            dir.join("data"),
        );
        config.endpoint = if cfg!(windows) {
            format!(r"\\.\pipe\m4tv-engine-test-{}-{n}", std::process::id())
        } else {
            dir.join("engine.sock").to_string_lossy().into_owned()
        };
        config.frames_dir = dir.join("frames");
        config.restart_delay = Duration::from_millis(50);
        config.env = vec![
            ("MARK".into(), dir.join("mark").into()),
            ("BEAT".into(), dir.join("beat").into()),
        ];
        Self { dir, endpoint: config.endpoint.clone(), config: Some(config) }
    }

    fn start(&mut self) -> ElectronEngine {
        ElectronEngine::start(self.config.take().unwrap()).unwrap()
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn frames(&self) -> PathBuf {
        self.dir.join("frames")
    }

    /// How many times the child has started, and how long its heartbeat is.
    fn starts(&self) -> usize {
        std::fs::read(self.dir.join("mark")).map_or(0, |m| m.len())
    }

    fn beats(&self) -> usize {
        std::fs::read(self.dir.join("beat")).map_or(0, |m| m.len())
    }
}

/// The host's end of the connection.
struct Host<S: AsyncRead + AsyncWrite> {
    lines: Lines<BufReader<tokio::io::ReadHalf<S>>>,
    writer: WriteHalf<S>,
}

#[cfg(unix)]
type Stream = tokio::net::UnixStream;
#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;

async fn connect(endpoint: &str) -> Host<Stream> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        #[cfg(unix)]
        let stream: io::Result<Stream> = Stream::connect(endpoint).await;
        #[cfg(windows)]
        let stream: io::Result<Stream> = tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint);
        match stream {
            Ok(stream) => {
                let (reader, writer) = tokio::io::split(stream);
                return Host { lines: BufReader::new(reader).lines(), writer };
            }
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => panic!("the Engine never listened on {endpoint}: {e}"),
        }
    }
}

impl<S: AsyncRead + AsyncWrite> Host<S> {
    /// The next message the Shell sent.
    async fn next(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(10), self.lines.next_line())
            .await
            .expect("the Engine said nothing for ten seconds")
            .unwrap()
            .expect("the Engine hung up");
        serde_json::from_str(&line).unwrap()
    }

    async fn say(&mut self, message: Value) {
        self.writer.write_all(format!("{message}\n").as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }
}

/// Waits until `done` holds, as the Engine works on its own thread.
async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn youtube() -> m4tv_shell::apps::AppDefinition {
    built_in_apps().into_iter().find(|app| app.id == "youtube").unwrap()
}

fn write_frame(dir: &Path, slot: u8, width: u32, height: u32, fill: u8) {
    std::fs::write(
        dir.join(format!("frame-{slot}.bgra")),
        vec![fill; width as usize * height as usize * 4],
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_an_app_tells_the_host_what_to_run_and_closing_it_says_so() {
    let mut rig = Rig::new(ALIVE);
    let mut engine = rig.start();
    let mut host = connect(&rig.endpoint()).await;

    engine.open(&youtube());
    let launch = host.next().await;
    assert_eq!(launch["t"], "launch");
    assert_eq!(launch["app"]["id"], "youtube");
    assert_eq!(launch["app"]["start_url"], "https://www.youtube.com/tv");
    assert!(launch["app"]["user_agent"].as_str().unwrap().contains("PlayStation"));
    assert!(launch["app"]["script"].as_str().unwrap().contains("m4tvSpatialNav"));
    assert_eq!((launch["width"].clone(), launch["height"].clone()), (json!(1280), json!(720)));

    // The other Apps use the Engine's own user agent.
    let netflix = built_in_apps().into_iter().find(|app| app.id == "netflix").unwrap();
    engine.open(&netflix);
    assert_eq!(host.next().await["app"]["user_agent"], Value::Null);

    engine.close();
    assert_eq!(host.next().await, json!({"t": "close"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_text_and_back_reach_the_host() {
    let mut rig = Rig::new(ALIVE);
    let mut engine = rig.start();
    let mut host = connect(&rig.endpoint()).await;
    until("the connection", || engine.connected()).await;

    engine.key(Key::Enter);
    engine.key(Key::Home); // the Shell's own: never sent
    engine.key(Key::PlayPause);
    engine.type_text("héllo \"world\"");
    engine.go_back();
    assert_eq!(host.next().await, json!({"t": "key", "key": "Return"}));
    assert_eq!(host.next().await, json!({"t": "key", "key": "MediaPlayPause"}));
    assert_eq!(host.next().await, json!({"t": "text", "text": "héllo \"world\""}));
    assert_eq!(host.next().await, json!({"t": "back"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_frame_is_read_from_its_slot_and_a_closed_app_or_a_bad_frame_is_refused() {
    let mut rig = Rig::new(ALIVE);
    let mut engine = rig.start();
    let mut host = connect(&rig.endpoint()).await;
    engine.open(&youtube());
    let session = host.next().await["session"].as_u64().unwrap();

    write_frame(&rig.frames(), 0, 4, 2, 7);
    host.say(json!({"t": "frame", "session": session, "slot": 0, "width": 4, "height": 2})).await;
    let mut frame = None;
    until("the frame", || {
        frame = engine.take_frame();
        frame.is_some()
    })
    .await;
    let frame = frame.unwrap();
    assert_eq!((frame.width, frame.height), (4, 2));
    assert_eq!(frame.bgra, vec![7; 32]);
    assert!(engine.take_frame().is_none(), "a frame is handed over once");

    // The wrong size, an unknown slot and a session that is over all leave nothing behind.
    host.say(json!({"t": "frame", "session": session, "slot": 0, "width": 9, "height": 9})).await;
    host.say(json!({"t": "frame", "session": session, "slot": 7, "width": 4, "height": 2})).await;
    engine.close();
    assert_eq!(host.next().await["t"], "close");
    host.say(json!({"t": "frame", "session": session, "slot": 0, "width": 4, "height": 2})).await;
    host.say(json!({"t": "ready"})).await; // the host speaks in order: this one has been handled
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(engine.take_frame().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_reports_an_editable_field_and_whether_the_page_can_go_back() {
    let mut rig = Rig::new(ALIVE);
    let mut engine = rig.start();
    let mut host = connect(&rig.endpoint()).await;
    engine.open(&youtube());
    let session = host.next().await["session"].as_u64().unwrap();
    assert!(!engine.editable_focused() && !engine.can_go_back());

    host.say(json!({"t": "state", "session": session, "can_go_back": true, "editable": true})).await;
    until("the state", || engine.editable_focused() && engine.can_go_back()).await;

    // A host that dies takes its page's state with it.
    drop(host);
    until("the state to clear", || !engine.editable_focused() && !engine.can_go_back()).await;
    let mut host = connect(&rig.endpoint()).await;
    assert_eq!(host.next().await["t"], "launch");

    // Another App's page has no say once this one is closed, and a new App starts clean.
    engine.close();
    host.say(json!({"t": "state", "session": session, "can_go_back": true, "editable": true})).await;
    engine.open(&youtube());
    assert!(!engine.editable_focused() && !engine.can_go_back());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_that_dies_is_started_again_and_launches_the_open_app_again() {
    let mut rig = Rig::new(ALIVE);
    let mut engine = rig.start();
    let mut host = connect(&rig.endpoint()).await;
    engine.open(&youtube());
    let first = host.next().await;
    until("the first host", || rig.starts() == 1).await;

    // The host's connection ends, as it does when Electron is killed.
    drop(host);
    until("the connection to drop", || !engine.connected()).await;
    let mut host = connect(&rig.endpoint()).await;
    let again = host.next().await;
    assert_eq!(again, first, "the page reloads where it was");
    until("a second host", || rig.starts() == 2).await;

    // A closed App is not launched again.
    engine.close();
    assert_eq!(host.next().await["t"], "close");
    drop(host);
    until("the connection to drop", || !engine.connected()).await;
    let mut host = connect(&rig.endpoint()).await;
    until("the connection", || engine.connected()).await;
    engine.key(Key::Left);
    assert_eq!(host.next().await["t"], "key");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_that_exits_is_started_again() {
    let mut rig = Rig::new(DIES);
    let _engine = rig.start();
    until("three starts", || rig.starts() >= 3).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_does_not_outlive_the_engine() {
    let mut rig = Rig::new(ALIVE);
    let engine = rig.start();
    until("the heartbeat", || rig.beats() > 3).await;
    drop(engine);

    tokio::time::sleep(Duration::from_millis(500)).await;
    let beats = rig.beats();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.beats(), beats, "the host is still running");
    assert!(!rig.frames().exists(), "the frames are cleaned up");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_that_never_connects_is_taken_for_hung_and_started_again() {
    let mut rig = Rig::new(ALIVE);
    rig.config.as_mut().unwrap().connect_timeout = Duration::from_millis(200);
    let _engine = rig.start();
    until("a second start", || rig.starts() >= 2).await;
}

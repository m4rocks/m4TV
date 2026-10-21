//! The real Engine: the pinned castLabs Electron running `engine/host`, against a page served from
//! here. It needs the Electron `just engine` fetches (or `M4TV_ELECTRON`), so it only runs when
//! asked: `cargo test -p m4tv-shell --test electron_host -- --ignored`.

use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use m4tv_shell::{
    apps::parse_catalog,
    electron_engine::{ElectronEngine, EngineConfig},
    engine::{Engine, Frame},
    shell::Key,
};

/// Paints red the first time it is loaded in a storage and yellow after that, turns green on the
/// Right key, and has a field with the focus.
const PAGE: &str = r#"<!doctype html><body style="margin:0">
<input autofocus style="position:absolute;opacity:0">
<script>
  document.body.style.background = localStorage.seen ? '#ff0' : '#f00';
  localStorage.seen = '1';
  addEventListener('keydown', (e) => { if (e.key === 'ArrowRight') document.body.style.background = '#0f0'; });
</script></body>"#;

/// Serves `PAGE` and remembers the user agent of every request.
fn serve() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let agents = Arc::new(Mutex::new(Vec::new()));
    let seen = agents.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]);
            if let Some(agent) = text.lines().find_map(|l| l.strip_prefix("User-Agent: ")) {
                seen.lock().unwrap().push(agent.trim().to_string());
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
                PAGE.len()
            );
        }
    });
    (port, agents)
}

fn electron() -> PathBuf {
    if let Some(program) = std::env::var_os("M4TV_ELECTRON") {
        return program.into();
    }
    let out = std::process::Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../engine/fetch-electron.mjs"))
        .output()
        .expect("node runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().into()
}

/// The colour in the middle of a frame, as [b, g, r, a].
fn centre(frame: &Frame) -> [u8; 4] {
    let at = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
    frame.bgra[at..at + 4].try_into().unwrap()
}

const RED: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [0, 255, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// Waits for a frame whose middle is `colour`.
fn paints(engine: &mut ElectronEngine, colour: [u8; 4]) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut last = None;
    while Instant::now() < deadline {
        if let Some(frame) = engine.take_frame() {
            last = Some(centre(&frame));
            if last == Some(colour) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no frame with {colour:?} in the middle; the last had {last:?}");
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn kill(pid: u32) {
    let pid = pid.to_string();
    let (program, args): (&str, &[&str]) =
        if cfg!(windows) { ("taskkill", &["/F", "/PID", &pid]) } else { ("kill", &["-9", &pid]) };
    assert!(std::process::Command::new(program).args(args).output().unwrap().status.success());
}

#[test]
#[ignore = "needs the Electron that `just engine` fetches"]
fn electron_paints_a_page_takes_keys_keeps_its_storage_and_comes_back_when_killed() {
    let (port, agents) = serve();
    let apps = parse_catalog(&format!(
        r#"
        [[app]]
        id = "one"
        name = "One"
        icon = "one"
        start_url = "http://127.0.0.1:{port}/"
        user_agent = "M4TV-Test-UA/1"

        [[app]]
        id = "two"
        name = "Two"
        icon = "two"
        start_url = "http://127.0.0.1:{port}/"
        "#
    ))
    .unwrap();
    let data = std::env::temp_dir().join(format!("m4tv-electron-host-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let host = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../engine/host");
    let mut engine =
        ElectronEngine::start(EngineConfig::new(electron(), vec![host.into()], data.clone()))
            .unwrap();

    // Frames, and the page's own scripts and focus.
    engine.open(&apps[0]);
    paints(&mut engine, RED);
    until("the focused field", || engine.editable_focused());
    assert_eq!(agents.lock().unwrap().first().map(String::as_str), Some("M4TV-Test-UA/1"));

    // Keys reach the page.
    engine.key(Key::Right);
    paints(&mut engine, GREEN);

    // The App's storage outlives its window; another App has its own.
    engine.close();
    engine.open(&apps[0]);
    paints(&mut engine, YELLOW);
    engine.open(&apps[1]);
    paints(&mut engine, RED);
    assert!(
        agents.lock().unwrap().last().unwrap().contains("Electron"),
        "an App without a user agent of its own uses the Engine's"
    );

    // Killing Electron brings it back, and the App reloads.
    engine.open(&apps[0]);
    paints(&mut engine, YELLOW);
    let first = engine.host_id().expect("the host runs");
    kill(first);
    until("a new host", || engine.host_id().is_some_and(|pid| pid != first) && engine.connected());
    paints(&mut engine, YELLOW);

    // Closing the App stops its window: no frame arrives afterwards.
    engine.close();
    std::thread::sleep(Duration::from_millis(500));
    let _ = engine.take_frame();
    std::thread::sleep(Duration::from_millis(500));
    assert!(engine.take_frame().is_none());

    drop(engine);
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
#[ignore = "needs the Electron that `just engine` fetches, and the internet"]
fn the_built_in_apps_paint_something_in_the_engine() {
    let data = std::env::temp_dir().join(format!("m4tv-electron-apps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let host = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../engine/host");
    let mut engine =
        ElectronEngine::start(EngineConfig::new(electron(), vec![host.into()], data.clone()))
            .unwrap();
    for app in m4tv_shell::apps::built_in_apps() {
        engine.open(&app);
        let deadline = Instant::now() + Duration::from_secs(90);
        // A page that has drawn more than a blank background has more than one colour in it.
        let drawn = loop {
            if let Some(frame) = engine.take_frame() {
                let first = frame.bgra[..4].to_vec();
                if frame.bgra.chunks(4).step_by(997).any(|pixel| pixel != first) {
                    break true;
                }
            }
            if Instant::now() > deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(drawn, "{} painted nothing but one colour", app.name);
        engine.close();
    }
    drop(engine);
    let _ = std::fs::remove_dir_all(&data);
}

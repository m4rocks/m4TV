//! The Shell. On Windows this is the Dev host: an ordinary window, a keyboard for the TV remote.

mod ui;

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use m4tv_os::{FakeOs, Os};
use m4tv_shell::{
    apps::built_in_apps,
    electron_engine::{ElectronEngine, EngineConfig},
    engine::Engine,
    remote::Remote,
    shell::{Screen, Shell},
};
use ui::ShellView;

/// Inter, bundled from `design/fonts` with its licence (SIL OFL 1.1).
const FONTS: [&[u8]; 2] = [
    include_bytes!("../../../design/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../design/fonts/Inter-Bold.ttf"),
];

/// The Electron that `node engine/fetch-electron.mjs` unpacks for this machine (ADR 0006), unless
/// `M4TV_ELECTRON` says otherwise.
fn electron_program(root: &Path) -> PathBuf {
    if let Some(program) = std::env::var_os("M4TV_ELECTRON") {
        return program.into();
    }
    let pin: serde_json::Value =
        serde_json::from_str(include_str!("../../../engine/electron.pin.json"))
            .expect("engine/electron.pin.json is JSON");
    let target = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
        .replace("x86_64", "x64")
        .replace("aarch64", "arm64")
        .replace("windows", "win32");
    let name = if cfg!(windows) { "electron.exe" } else { "electron" };
    root.join(".m4tv-dev/electron").join(format!("{}-{target}", pin["tag"].as_str().unwrap())).join(name)
}

/// Starts the Engine: the pinned Electron running `engine/host`. Without it the Shell still runs,
/// with Apps that show nothing.
fn start_engine(data: PathBuf) -> Box<dyn Engine> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let program = electron_program(&root);
    if !program.exists() {
        eprintln!("Engine not started: {} is missing; run `node engine/fetch-electron.mjs`", program.display());
        return Box::new(m4tv_shell::engine::NoEngine);
    }
    let host = std::env::var_os("M4TV_ENGINE_HOST")
        .map_or_else(|| root.join("engine/host"), PathBuf::from);
    let config = EngineConfig::new(program, vec![host.into()], data);
    match ElectronEngine::start(config) {
        Ok(engine) => Box::new(engine),
        Err(e) => {
            eprintln!("Engine not started: {e}");
            Box::new(m4tv_shell::engine::NoEngine)
        }
    }
}

/// The Dev host's Shell, with its Remote server. Without a Remote it still runs.
fn dev_host_shell() -> Shell {
    let os = FakeOs::dev_host();
    let engine = start_engine(os.engine_data_path());
    let mut shell = Shell::new(built_in_apps(), Box::new(os), engine);
    // Port 80 is for the Pi; the Dev host's hotspot listener takes an unprivileged one.
    match Remote::start(&Path::new(".m4tv-dev").join("remote"), 8443, true) {
        Ok(remote) => shell.attach_remote(remote.with_hotspot_port(8080)),
        Err(e) => eprintln!("Remote server not started: {e}"),
    }
    shell
}

fn main() {
    application().run(|cx: &mut App| {
        cx.text_system()
            .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
            .expect("the bundled Inter loads");
        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx| {
                        let focus = cx.focus_handle();
                        window.focus(&focus, cx);
                        ShellView::new(dev_host_shell(), focus)
                    })
                },
            )
            .unwrap();
        // The Engine works on this thread, so it gets turns here; an open App repaints each turn.
        cx.spawn(async move |cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let elapsed = last.elapsed();
                last = Instant::now();
                let _ = window.update(cx, |view, _window, cx| {
                    // Redraw while an App plays, and when a tick changed anything else.
                    let changed = view.shell.tick(elapsed);
                    if matches!(view.shell.screen(), Screen::App(_)) || changed {
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.activate(true);
    });
}

//! The Shell. On Windows this is the Dev host: an ordinary window, a keyboard for the TV remote.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, Context, FocusHandle, FontWeight, KeyDownEvent, RenderImage, Window,
    WindowBounds, WindowOptions, div, img, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use m4tv_os::FakeOs;
use m4tv_shell::{
    apps::built_in_apps,
    electron_engine::{ElectronEngine, EngineConfig},
    engine::{Engine, Frame},
    remote::Remote,
    shell::{Key, Overlay, Row, Screen, Shell},
    tokens::{self, DARK as PALETTE},
};

/// Inter, bundled from `design/fonts` with its licence (SIL OFL 1.1).
const FONTS: [&[u8]; 2] = [
    include_bytes!("../../../design/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../design/fonts/Inter-Bold.ttf"),
];

struct ShellView {
    shell: Shell,
    focus: FocusHandle,
    /// The App's newest frame as gpui draws it.
    page: Option<Arc<RenderImage>>,
}

fn render_image(frame: Frame) -> RenderImage {
    // gpui's RenderImage is BGRA, which is what the Engine hands over, so the bytes go in as is.
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)
        .expect("a frame holds width * height * 4 bytes");
    RenderImage::new(vec![image::Frame::new(buffer)])
}

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
fn start_engine() -> Box<dyn Engine> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let program = electron_program(&root);
    if !program.exists() {
        eprintln!("Engine not started: {} is missing; run `node engine/fetch-electron.mjs`", program.display());
        return Box::new(m4tv_shell::engine::NoEngine);
    }
    let host = std::env::var_os("M4TV_ENGINE_HOST")
        .map_or_else(|| root.join("engine/host"), PathBuf::from);
    let config = EngineConfig::new(program, vec![host.into()], ".m4tv-dev/electron-data".into());
    match ElectronEngine::start(config) {
        Ok(engine) => Box::new(engine),
        Err(e) => {
            eprintln!("Engine not started: {e}");
            Box::new(m4tv_shell::engine::NoEngine)
        }
    }
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if let Some(key) = Key::from_keystroke(&event.keystroke.key) {
                    this.shell.press(key);
                    cx.notify();
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(tokens::SPACE_8))
            .font_family("Inter")
            .bg(rgb(PALETTE.background))
            .text_color(rgb(PALETTE.text));

        let screen = self.shell.screen().clone();
        if !matches!(screen, Screen::App(_))
            && let Some(old) = self.page.take()
        {
            let _ = window.drop_image(old);
        }
        let root = match screen {
            Screen::Home => {
                let labels = self.shell.apps().iter().map(|app| app.name.clone());
                let tiles = labels.chain(["Settings".to_string()]).enumerate().map(|(i, label)| {
                    let focused = i == self.shell.focused();
                    div()
                        .w(px(240.))
                        .h(px(150.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(tokens::RADIUS_MEDIUM))
                        .border_4()
                        .border_color(if focused { rgb(PALETTE.ink) } else { rgb(PALETTE.border) })
                        .bg(if focused { rgb(PALETTE.raised) } else { rgb(PALETTE.surface) })
                        .text_size(px(tokens::TEXT_2XL))
                        .child(label)
                });
                root.child(heading("m4TV"))
                    .child(div().flex().gap(px(tokens::SPACE_6)).children(tiles))
            }
            Screen::Settings => {
                let rows = self.shell.settings_rows().into_iter().enumerate().map(|(i, row)| {
                    let focused = i == self.shell.settings_focused();
                    let value = match row {
                        Row::Setting(def) => self.shell.setting_text(def.key),
                        _ => row.hint().to_string(),
                    };
                    div()
                        .w(px(480.))
                        .px(px(tokens::SPACE_6))
                        .py(px(tokens::SPACE_3))
                        .flex()
                        .justify_between()
                        .rounded(px(tokens::RADIUS_MEDIUM))
                        .border_2()
                        .border_color(if focused { rgb(PALETTE.ink) } else { rgb(PALETTE.border) })
                        .bg(if focused { rgb(PALETTE.raised) } else { rgb(PALETTE.surface) })
                        .text_size(px(tokens::TEXT_XL))
                        .child(row.label())
                        .child(value)
                });
                root.child(heading("Settings"))
                    .child(div().flex().flex_col().gap(px(tokens::SPACE_3)).children(rows))
                    .child(
                        div()
                            .text_color(rgb(PALETTE.muted))
                            .child(format!("Version {}", self.shell.version())),
                    )
                    .child(div().h(px(28.)).child(self.shell.notice().unwrap_or("").to_string()))
            }
            Screen::App(id) => {
                if let Some(frame) = self.shell.take_frame() {
                    // Each frame is a new image; the atlas keeps it until it is dropped.
                    let new = Arc::new(render_image(frame));
                    if let Some(old) = self.page.replace(new) {
                        let _ = window.drop_image(old);
                    }
                }
                match &self.page {
                    Some(page) => root.child(img(page.clone()).size_full()),
                    None => root.child(heading(
                        self.shell.app(&id).expect("open App is in the catalogue").name.clone(),
                    )),
                }
            }
        };
        // Overlays sit on top of the screen, the playing App included, and take no focus.
        root.children(self.shell.overlay().map(|overlay| {
            match overlay {
                Overlay::Volume { level, muted } => div()
                    .absolute()
                    .top(px(tokens::SPACE_8))
                    .right(px(tokens::SPACE_8))
                    .px(px(tokens::SPACE_6))
                    .py(px(tokens::SPACE_3))
                    .rounded(px(tokens::RADIUS_MEDIUM))
                    .bg(rgb(PALETTE.surface))
                    .border_2()
                    .border_color(rgb(PALETTE.border))
                    .text_size(px(tokens::TEXT_2XL))
                    .child(if *muted { "Muted".to_string() } else { format!("Volume {level}") }),
                Overlay::Pairing { pin, payload, cert_replaced } => panel()
                    .child(div().text_size(px(tokens::TEXT_XL)).child("Pair your phone"))
                    .child(qr_code(payload))
                    .child(div().text_size(px(tokens::TEXT_2XL)).child(format!("PIN {pin}")))
                    .children(cert_replaced.then(|| {
                        div()
                            .text_color(rgb(PALETTE.state))
                            .child("This TV's certificate changed: pair your phones again")
                    })),
                Overlay::Hotspot { ssid, password, payload, url, status } => panel()
                    .child(
                        div().text_size(px(tokens::TEXT_XL)).child("Set up this TV: join its Wi-Fi"),
                    )
                    .child(qr_code(payload))
                    .child(div().text_size(px(tokens::TEXT_2XL)).child(ssid.clone()))
                    .child(div().text_size(px(tokens::TEXT_2XL)).child(format!("Password {password}")))
                    .children((!url.is_empty()).then(|| div().child(format!("Then open {url}"))))
                    .children(
                        (!status.is_empty())
                            .then(|| div().text_color(rgb(PALETTE.state)).child(status.clone())),
                    ),
                Overlay::Joined { address, payload } => panel()
                    .child(div().text_size(px(tokens::TEXT_XL)).child("Connected: pair your phone"))
                    .child(qr_code(payload))
                    .child(div().text_size(px(tokens::TEXT_2XL)).child(format!("https://{address}")))
                    .child(div().child("or https://m4tv.local")),
            }
        }))
    }
}

/// A screen's title.
fn heading(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div().text_size(px(tokens::TEXT_3XL)).font_weight(FontWeight::BOLD).child(text.into())
}

/// The box an Overlay with a QR code sits in.
fn panel() -> gpui::Div {
    div()
        .absolute()
        .right(px(tokens::SPACE_8))
        .bottom(px(tokens::SPACE_8))
        .p(px(tokens::SPACE_6))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(tokens::SPACE_3))
        .rounded(px(tokens::RADIUS_MEDIUM))
        .bg(rgb(PALETTE.surface))
        .border_2()
        .border_color(rgb(PALETTE.border))
}

fn qr_code(payload: &str) -> gpui::Div {
    let code = qrcode::QrCode::new(payload.as_bytes()).expect("a link fits in a QR code");
    let width = code.width();
    let modules = code.to_colors();
    let rows = modules.chunks(width).map(|row| {
        div().flex().children(row.iter().map(|module| {
            let dark = *module == qrcode::Color::Dark;
            div().size(px(6.)).bg(if dark { rgb(tokens::QR_DARK) } else { rgb(tokens::QR_LIGHT) })
        }))
    });
    div().p(px(tokens::SPACE_3)).bg(rgb(tokens::QR_LIGHT)).flex().flex_col().children(rows.collect::<Vec<_>>())
}

/// The Dev host's Shell, with its Remote server. Without a Remote it still runs.
fn dev_host_shell(engine: Box<dyn Engine>) -> Shell {
    let mut shell = Shell::new(built_in_apps(), Box::new(FakeOs::dev_host()), engine);
    // Port 80 is for the Pi; the Dev host's hotspot listener takes an unprivileged one.
    match Remote::start(&Path::new(".m4tv-dev").join("remote"), 8443, true) {
        Ok(remote) => shell.attach_remote(remote.with_hotspot_port(8080)),
        Err(e) => eprintln!("Remote server not started: {e}"),
    }
    shell
}

fn main() {
    let engine = start_engine();
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
                        ShellView { shell: dev_host_shell(engine), focus, page: None }
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

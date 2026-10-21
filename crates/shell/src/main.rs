//! The Shell. On Windows this is the Dev host: an ordinary window, a keyboard for the TV remote.

#[cfg(feature = "cef")]
mod cef_engine;

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, Context, FocusHandle, KeyDownEvent, RenderImage, Window, WindowBounds,
    WindowOptions, div, img, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use m4tv_os::FakeOs;
use m4tv_shell::{
    apps::built_in_apps,
    engine::{Engine, Frame},
    remote::Remote,
    shell::{Key, Overlay, Row, Screen, Shell},
};

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

/// Starts the Engine. With CEF, a helper process exits in here and never reaches the Shell.
#[cfg(feature = "cef")]
fn start_engine() -> Box<dyn Engine> {
    Box::new(cef_engine::start(".m4tv-dev/cef".into()))
}

#[cfg(not(feature = "cef"))]
fn start_engine() -> Box<dyn Engine> {
    Box::new(m4tv_shell::engine::NoEngine)
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
            .gap_8()
            .bg(rgb(0x101418))
            .text_color(rgb(0xffffff));

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
                        .rounded_lg()
                        .border_4()
                        .border_color(if focused { rgb(0xffffff) } else { rgb(0x2a323c) })
                        .bg(if focused { rgb(0x2a323c) } else { rgb(0x1a2028) })
                        .text_2xl()
                        .child(label)
                });
                root.child(div().text_3xl().child("m4TV"))
                    .child(div().flex().gap_6().children(tiles))
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
                        .px_6()
                        .py_3()
                        .flex()
                        .justify_between()
                        .rounded_lg()
                        .border_2()
                        .border_color(if focused { rgb(0xffffff) } else { rgb(0x2a323c) })
                        .bg(if focused { rgb(0x2a323c) } else { rgb(0x1a2028) })
                        .text_xl()
                        .child(row.label())
                        .child(value)
                });
                root.child(div().text_3xl().child("Settings"))
                    .child(div().flex().flex_col().gap_3().children(rows))
                    .child(
                        div()
                            .text_color(rgb(0x8a96a3))
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
                    None => root.child(div().text_3xl().child(
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
                    .top(px(32.))
                    .right(px(32.))
                    .px_6()
                    .py_3()
                    .rounded_lg()
                    .bg(rgb(0x1a2028))
                    .border_2()
                    .border_color(rgb(0x2a323c))
                    .text_2xl()
                    .child(if *muted { "Muted".to_string() } else { format!("Volume {level}") }),
                Overlay::Pairing { pin, payload, cert_replaced } => panel()
                    .child(div().text_xl().child("Pair your phone"))
                    .child(qr_code(payload))
                    .child(div().text_2xl().child(format!("PIN {pin}")))
                    .children(cert_replaced.then(|| {
                        div()
                            .text_color(rgb(0xf0b429))
                            .child("This TV's certificate changed: pair your phones again")
                    })),
                Overlay::Hotspot { ssid, password, payload, status } => panel()
                    .child(div().text_xl().child("Set up this TV: join its Wi-Fi"))
                    .child(qr_code(payload))
                    .child(div().text_2xl().child(ssid.clone()))
                    .child(div().text_2xl().child(format!("Password {password}")))
                    .child(div().child("Then open http://m4tv.local"))
                    .children(
                        (!status.is_empty())
                            .then(|| div().text_color(rgb(0xf0b429)).child(status.clone())),
                    ),
                Overlay::Joined { address, payload } => panel()
                    .child(div().text_xl().child("Connected: pair your phone"))
                    .child(qr_code(payload))
                    .child(div().text_2xl().child(format!("https://{address}")))
                    .child(div().child("or https://m4tv.local")),
            }
        }))
    }
}

/// The box an Overlay with a QR code sits in.
fn panel() -> gpui::Div {
    div()
        .absolute()
        .right(px(32.))
        .bottom(px(32.))
        .p_6()
        .flex()
        .flex_col()
        .items_center()
        .gap_3()
        .rounded_lg()
        .bg(rgb(0x1a2028))
        .border_2()
        .border_color(rgb(0x2a323c))
}

fn qr_code(payload: &str) -> gpui::Div {
    let code = qrcode::QrCode::new(payload.as_bytes()).expect("a link fits in a QR code");
    let width = code.width();
    let modules = code.to_colors();
    let rows = modules.chunks(width).map(|row| {
        div().flex().children(row.iter().map(|module| {
            let dark = *module == qrcode::Color::Dark;
            div().size(px(6.)).bg(if dark { rgb(0x000000) } else { rgb(0xffffff) })
        }))
    });
    div().p_3().bg(rgb(0xffffff)).flex().flex_col().children(rows.collect::<Vec<_>>())
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
                #[cfg(feature = "cef")]
                cef_engine::pump();
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
    #[cfg(feature = "cef")]
    cef_engine::shutdown_cef();
}

//! The Shell's screens, drawn from the design tokens (ADR 0005, `design/tokens.json`).

mod home;
mod logos;
mod overlay;
mod settings;

use std::sync::Arc;

use gpui::{
    Context, FocusHandle, Hsla, KeyDownEvent, RenderImage, Window, div, img, prelude::*, px, rgb,
};
use m4tv_shell::{
    engine::Frame,
    shell::{Key, Screen, Shell, Theme},
    tokens::{self, Palette},
};

/// A token colour, 0xRRGGBB.
fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => tokens::DARK,
        Theme::Light => tokens::LIGHT,
    }
}

pub struct ShellView {
    pub shell: Shell,
    focus: FocusHandle,
    logos: logos::Logos,
    /// The App's newest frame as gpui draws it.
    page: Option<Arc<RenderImage>>,
}

impl ShellView {
    pub fn new(shell: Shell, focus: FocusHandle) -> Self {
        Self { shell, focus, logos: Default::default(), page: None }
    }
}

fn render_image(frame: Frame) -> RenderImage {
    // gpui's RenderImage is BGRA, which is what the Engine hands over, so the bytes go in as is.
    let buffer = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)
        .expect("a frame holds width * height * 4 bytes");
    RenderImage::new(vec![image::Frame::new(buffer)])
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(self.shell.theme());
        // The Safe area: text and Tiles stay inside it, whatever the TV's overscan does.
        let margin = px(f32::from(window.viewport_size().width) * tokens::SAFE_AREA);
        let root = div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if let Some(key) = Key::from_keystroke(&event.keystroke.key) {
                    this.shell.press(key);
                    cx.notify();
                }
            }))
            .size_full()
            .font_family("Inter")
            .bg(color(p.background))
            .text_color(color(p.text));

        let screen = self.shell.screen().clone();
        if !matches!(screen, Screen::App(_))
            && let Some(old) = self.page.take()
        {
            let _ = window.drop_image(old);
        }
        let root = match screen {
            Screen::Home => root.child(home::home(&self.shell, &mut self.logos, p, margin)),
            Screen::Settings => root.child(settings::settings(&self.shell, p, margin)),
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
                    None => root.child(
                        div().size_full().flex().items_center().justify_center().text_size(px(tokens::TEXT_3XL)).child(
                            self.shell.app(&id).expect("open App is in the catalogue").name.clone(),
                        ),
                    ),
                }
            }
        };
        // Overlays sit on top of the screen, the playing App included, and take no focus.
        root.children(self.shell.overlay().map(|o| overlay::overlay(o, p, margin)))
    }
}

//! The Shell's screens, drawn from the design tokens (ADR 0005, `design/tokens.json`).

mod home;
mod logos;
mod overlay;
mod settings;
mod transition;

use std::{sync::Arc, time::Instant};

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
    /// The App whose launch or leave transition is running, and how it is going.
    transition: Option<(String, transition::Transition)>,
}

impl ShellView {
    /// Starts the transition of an App that was opened, and ends the one of an App that was left.
    fn follow_screen(&mut self, screen: &Screen, now: Instant, window: &mut Window) {
        match (screen, &mut self.transition) {
            (Screen::App(id), current) if current.as_ref().is_none_or(|(open, t)| open != id || t.is_leaving()) => {
                // A new App starts clean: the last one's picture is not its own.
                if let Some(old) = self.page.take() {
                    let _ = window.drop_image(old);
                }
                *current = Some((id.clone(), transition::Transition::new(now, self.shell.reduce_motion())));
            }
            (Screen::App(_), _) => {}
            // Settings is only reached from Home, once the Tile is back.
            (Screen::Settings, current) => *current = None,
            (_, Some((_, transition))) => transition.leave(now),
            (_, None) => {}
        }
    }

    pub fn new(shell: Shell, focus: FocusHandle) -> Self {
        Self { shell, focus, logos: Default::default(), page: None, transition: None }
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
        let now = Instant::now();
        self.follow_screen(&screen, now, window);
        if let Screen::App(_) = screen
            && let Some(frame) = self.shell.take_frame()
        {
            // Each frame is a new image; the atlas keeps it until it is dropped.
            let new = Arc::new(render_image(frame));
            if let Some(old) = self.page.replace(new) {
                let _ = window.drop_image(old);
            }
            if let Some((_, transition)) = &mut self.transition {
                transition.painted(now);
            }
        }

        let mut look = self.transition.as_ref().map(|(_, t)| t.look(now));
        // A finished transition stays while its App plays, to be reversed by leaving; once Home
        // is back it ends, and the App's last picture goes with it.
        if look.as_ref().is_some_and(|look| look.done) && !matches!(screen, Screen::App(_)) {
            self.transition = None;
            look = None;
        }
        if !matches!(screen, Screen::App(_))
            && self.transition.is_none()
            && let Some(old) = self.page.take()
        {
            let _ = window.drop_image(old);
        }

        let root = match (&screen, look) {
            // Playing, once the transition has handed over.
            (Screen::App(id), look) if look.as_ref().is_none_or(|look| look.done) => match &self.page {
                Some(page) => root.child(img(page.clone()).size_full()),
                None => root.child(
                    div().size_full().flex().items_center().justify_center().text_size(px(tokens::TEXT_3XL)).child(
                        self.shell.app(id).expect("open App is in the catalogue").name.clone(),
                    ),
                ),
            },
            // Opening or leaving: Home underneath, the grown Tile over it, the App over that.
            (_, Some(look)) => {
                let (id, _) = self.transition.as_ref().expect("a look needs a transition");
                let viewport = window.viewport_size();
                let index = self.shell.apps().iter().position(|app| app.id == *id).unwrap_or(0);
                let from = home::tile_bounds(&self.shell, viewport, margin, index);
                let (id, theme) = (id.clone(), self.shell.theme());
                let name = self.shell.app(&id).map(|app| app.name.clone()).unwrap_or_default();
                let logo = self.logos.get(&id, theme);
                // Held on the logo, nothing moves until the Engine's first frame.
                if look.grow < 1. || look.cover < 1. || look.app > 0. || self.transition.as_ref().is_some_and(|(_, t)| t.is_leaving()) {
                    window.request_animation_frame();
                }
                let cover = home::cover(look.grow, look.cover, from, viewport, logo, &name, p);
                root.child(home::home(&self.shell, &mut self.logos, p, margin))
                    .child(cover)
                    .children(self.page.clone().map(|page| {
                        div().absolute().top_0().left_0().size_full().opacity(look.app).child(img(page).size_full())
                    }))
            }
            (Screen::Home | Screen::App(_), None) => root.child(home::home(&self.shell, &mut self.logos, p, margin)),
            (Screen::Settings, None) => root.child(settings::settings(&self.shell, p, margin)),
        };
        // Overlays sit on top of the screen, the playing App included, and take no focus.
        root.children(self.shell.overlay().map(|o| overlay::overlay(o, p, margin)))
    }
}

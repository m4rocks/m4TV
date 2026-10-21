//! Home: the Hero (clock and status) over a row of Tiles, the focused one grown and named.

use std::{sync::Arc, time::Duration};

use gpui::{
    Animation, AnimationExt, Bounds, BoxShadow, Div, ObjectFit, Pixels, RenderImage, Size, div,
    ease_out_quint, hsla, img, point, prelude::*, px, size,
};
use m4tv_shell::{
    shell::Shell,
    tokens::{self, Palette},
};

use super::{color, logos::Logos};

const TILE_W: f32 = 240.;
const TILE_H: f32 = 150.;
/// How much bigger the focused Tile is (ADR 0005).
const GROWTH: f32 = 0.08;

pub fn home(shell: &Shell, logos: &mut Logos, p: Palette, margin: Pixels) -> Div {
    let theme = shell.theme();
    let reduce = shell.reduce_motion();
    let mut tiles: Vec<_> = shell
        .apps()
        .iter()
        .map(|app| (app.name.clone(), Some(logos.get(&app.id, theme))))
        .collect();
    tiles.push(("Settings".to_string(), None));

    let slots = tiles.into_iter().enumerate().map(|(i, (name, logo))| {
        let focused = i == shell.focused();
        let face = div()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(tokens::RADIUS_LARGE))
            .border_2()
            .bg(color(p.surface))
            .child(match logo {
                Some(Some(logo)) => div()
                    .child(img(logo).w(px(TILE_W * 0.6)).h(px(TILE_H * 0.5)).object_fit(ObjectFit::Contain)),
                Some(None) => div().text_size(px(tokens::TEXT_XL)).child(name.clone()),
                None => settings_glyph(p),
            });
        let tile = if focused {
            let ink = p.ink;
            let growth = if reduce { 0. } else { GROWTH };
            let face = face.border_color(color(ink));
            // Moving the focus changes the id, which starts the motion again.
            face.with_animation(
                ("focus", i),
                Animation::new(Duration::from_millis(tokens::MOTION_FOCUS_MS))
                    .with_easing(ease_out_quint()),
                move |face, delta| {
                    let grown = 1. + growth * delta;
                    face.w(px(TILE_W * grown))
                        .h(px(TILE_H * grown))
                        .border_color(color(ink).opacity(delta))
                        .shadow(vec![BoxShadow {
                            color: hsla(0., 0., 0., 0.35 * delta),
                            offset: point(px(0.), px(tokens::SPACE_2)),
                            blur_radius: px(tokens::SPACE_6),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                },
            )
            .into_any_element()
        } else {
            face.w(px(TILE_W)).h(px(TILE_H)).border_color(color(p.surface)).into_any_element()
        };
        // The slot is as big as a grown Tile, so the others do not move when the focus does.
        div()
            .flex_none()
            .w(px(TILE_W * (1. + GROWTH)))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .h(px(TILE_H * (1. + GROWTH)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(tile),
            )
            .child(
                div()
                    .h(px(tokens::SPACE_8))
                    .pt(px(tokens::SPACE_2))
                    .text_size(px(tokens::TEXT_LG))
                    .text_color(color(p.text))
                    .children(focused.then_some(name)),
            )
    });

    div()
        .size_full()
        .flex()
        .flex_col()
        .p(margin)
        .child(hero(shell, p))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(tokens::SPACE_4))
                .children(slots),
        )
}

/// The clock, once the time is known, and a line saying how the TV is connected.
fn hero(shell: &Shell, p: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(tokens::SPACE_1))
        .children(shell.clock_text().map(|clock| {
            div().text_size(px(tokens::TEXT_3XL)).font_weight(gpui::FontWeight::BOLD).child(clock)
        }))
        .child(
            div()
                .text_size(px(tokens::TEXT_LG))
                .text_color(color(p.muted))
                .child(shell.status_line()),
        )
}

/// Settings has no logo: three bars, drawn.
fn settings_glyph(p: Palette) -> Div {
    div().flex().flex_col().gap(px(tokens::SPACE_2)).children((0..3).map(|_| {
        div().w(px(56.)).h(px(6.)).rounded(px(tokens::RADIUS_SMALL)).bg(color(p.text))
    }))
}

/// Where the Tile at `index` is on Home, for the launch transition to start from. It follows the
/// layout above by arithmetic, not by measuring.
// shortcut: the Hero's height is a constant, so a change to its text sizes needs it updated here.
pub fn tile_bounds(
    shell: &Shell,
    viewport: Size<Pixels>,
    margin: Pixels,
    index: usize,
) -> Bounds<Pixels> {
    // The launching Tile is the focused one, which is bigger unless motion is reduced.
    let grown = if shell.reduce_motion() { 1. } else { 1. + GROWTH };
    let (tile_w, tile_h) = (TILE_W * grown, TILE_H * grown);
    let (width, height, margin) = (f32::from(viewport.width), f32::from(viewport.height), f32::from(margin));
    let slot = TILE_W * (1. + GROWTH);
    let count = shell.apps().len() + 1;
    let row = count as f32 * slot + (count - 1) as f32 * tokens::SPACE_4;
    let hero = if shell.clock_text().is_some() { 68. } else { 28. };
    let column = TILE_H * (1. + GROWTH) + tokens::SPACE_8;
    let centre_x = (width - row) / 2. + index as f32 * (slot + tokens::SPACE_4) + slot / 2.;
    let centre_y = margin + hero + (height - 2. * margin - hero - column) / 2. + TILE_H * (1. + GROWTH) / 2.;
    Bounds::new(
        point(px(centre_x - tile_w / 2.), px(centre_y - tile_h / 2.)),
        size(px(tile_w), px(tile_h)),
    )
}

/// The Tile on its way to filling the screen, holding the App's logo, or the name without one.
pub fn cover(
    grow: f32,
    fade: f32,
    from: Bounds<Pixels>,
    viewport: Size<Pixels>,
    logo: Option<Arc<RenderImage>>,
    name: &str,
    p: Palette,
) -> Div {
    let lerp = |a: Pixels, b: f32| px(f32::from(a) + (b - f32::from(a)) * grow);
    div()
        .absolute()
        .left(lerp(from.origin.x, 0.))
        .top(lerp(from.origin.y, 0.))
        .w(lerp(from.size.width, f32::from(viewport.width)))
        .h(lerp(from.size.height, f32::from(viewport.height)))
        .rounded(px(tokens::RADIUS_LARGE * (1. - grow)))
        .opacity(fade)
        .flex()
        .items_center()
        .justify_center()
        .bg(color(p.surface))
        .text_size(px(tokens::TEXT_XL))
        .text_color(color(p.text))
        .child(match logo {
            Some(logo) => div()
                .child(img(logo).w(px(TILE_W * 0.6)).h(px(TILE_H * 0.5)).object_fit(ObjectFit::Contain)),
            None => div().child(name.to_string()),
        })
}

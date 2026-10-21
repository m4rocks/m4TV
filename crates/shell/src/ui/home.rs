//! Home: the Hero (clock and status) over a row of Tiles, the focused one grown and named.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, BoxShadow, Div, ObjectFit, Pixels, div, ease_out_quint, hsla, img,
    point, prelude::*, px,
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

//! Overlays: panels over the screen, the playing App included, and the full-screen Setup takeover.

use std::sync::Arc;

use gpui::{Div, FontFeatures, FontWeight, Pixels, div, prelude::*, px};
use m4tv_shell::{
    shell::Overlay,
    tokens::{self, Palette},
};

use super::color;

/// An Overlay panel: the surface colour at about 90%, no blur (ADR 0005).
fn panel(p: Palette) -> Div {
    div()
        .p(px(tokens::SPACE_6))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(tokens::SPACE_3))
        .rounded(px(tokens::RADIUS_LARGE))
        .bg(color(p.surface).opacity(tokens::OVERLAY_ALPHA))
        .border_2()
        .border_color(color(p.border))
        .text_color(color(p.text))
}

/// Figures of one width, so a PIN or a level does not jiggle.
fn tabular(text: Div) -> Div {
    text.font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
}

pub fn overlay(overlay: &Overlay, p: Palette, margin: Pixels) -> Div {
    let corner = |top: bool| {
        let at = div().absolute().right(margin);
        if top { at.top(margin) } else { at.bottom(margin) }
    };
    match overlay {
        Overlay::Volume { level, muted } => {
            let filled = if *muted { 0. } else { *level as f32 / 100. };
            corner(true).child(
                panel(p)
                    .child(tabular(div().text_size(px(tokens::TEXT_2XL)).child(if *muted {
                        "Muted".to_string()
                    } else {
                        format!("Volume {level}")
                    })))
                    .child(
                        div().w(px(240.)).h(px(tokens::SPACE_2)).rounded(px(tokens::RADIUS_SMALL)).bg(color(p.raised)).child(
                            div()
                                .w(gpui::relative(filled))
                                .h_full()
                                .rounded(px(tokens::RADIUS_SMALL))
                                .bg(color(p.ink)),
                        ),
                    ),
            )
        }
        Overlay::Pairing { pin, payload, cert_replaced } => corner(false).child(
            panel(p)
                .child(div().text_size(px(tokens::TEXT_XL)).child("Pair your phone"))
                .child(qr_code(payload, 6., tokens::SPACE_3))
                .child(tabular(div().text_size(px(tokens::TEXT_2XL)).child(format!("PIN {pin}"))))
                .children(cert_replaced.then(|| {
                    div()
                        .text_color(color(p.state))
                        .child("This TV's certificate changed: pair your phones again")
                })),
        ),
        Overlay::Cdm { reason } => div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(match reason {
                None => panel(p)
                    .child(div().text_size(px(tokens::TEXT_2XL)).child("Preparing video playback…"))
                    .child(
                        div()
                            .text_size(px(tokens::TEXT_LG))
                            .text_color(color(p.muted))
                            .child("Back to cancel"),
                    ),
                Some(reason) => panel(p)
                    .child(div().text_size(px(tokens::TEXT_2XL)).child("Video playback is not ready"))
                    .child(failure(p, reason))
                    .child(
                        div()
                            .text_size(px(tokens::TEXT_LG))
                            .text_color(color(p.muted))
                            .child("Enter to retry · Back to cancel"),
                    ),
            }),
        Overlay::Hotspot { ssid, password, payload, url, .. } => {
            let steps = [
                (format!("Join the Wi-Fi {ssid}"), Some(format!("Password {password}"))),
                (
                    if url.is_empty() {
                        "Open the page your phone offers".to_string()
                    } else {
                        format!("Open {url}")
                    },
                    None,
                ),
            ];
            takeover(p, "Set up this TV", payload, steps, overlay.hotspot_joining(), overlay.hotspot_failure())
        }
        Overlay::Joined { address, payload } => takeover(
            p,
            "Connected",
            payload,
            [
                ("Scan this code to pair your phone".to_string(), None),
                (format!("Or open https://{address}"), Some("or https://m4tv.local".to_string())),
            ],
            None,
            None,
        ),
    }
}

/// An error in the state colour: a badge and words, never colour alone.
fn failure(p: Palette, reason: &str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(tokens::SPACE_3))
        .text_size(px(tokens::TEXT_XL))
        .text_color(color(p.state))
        .child(
            div()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_2()
                .border_color(color(p.state))
                .font_weight(FontWeight::BOLD)
                .child("!"),
        )
        .child(reason.to_string())
}

/// The first-boot screen, which fills the display: a large code on a white card, and numbered steps.
fn takeover(
    p: Palette,
    title: &str,
    payload: &str,
    steps: [(String, Option<String>); 2],
    progress: Option<&str>,
    problem: Option<&str>,
) -> Div {
    let steps = steps.into_iter().enumerate().map(|(i, (text, detail))| {
        div()
            .flex()
            .items_start()
            .gap(px(tokens::SPACE_4))
            .child(
                div()
                    .flex_none()
                    .size(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(color(p.ink))
                    .text_color(color(p.background))
                    .text_size(px(tokens::TEXT_XL))
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{}", i + 1)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(tokens::TEXT_2XL)).child(text))
                    .children(detail.map(|detail| {
                        tabular(div().text_size(px(tokens::TEXT_XL)).text_color(color(p.muted)).child(detail))
                    })),
            )
    });
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(tokens::SPACE_8))
        .bg(color(p.background))
        .text_color(color(p.text))
        .child(qr_code(payload, 10., tokens::SPACE_6))
        .child(
            div()
                .max_w(px(560.))
                .flex()
                .flex_col()
                .gap(px(tokens::SPACE_6))
                .child(div().text_size(px(tokens::TEXT_3XL)).font_weight(FontWeight::BOLD).child(title.to_string()))
                .children(steps)
                .children(progress.map(|text| {
                    div().text_size(px(tokens::TEXT_XL)).text_color(color(p.muted)).child(text.to_string())
                }))
                .children(problem.map(|reason| failure(p, reason))),
        )
}

/// A QR code, black on a white card in every theme so a phone reads it from across the room.
fn qr_code(payload: &str, module: f32, padding: f32) -> Div {
    let code = qrcode::QrCode::new(payload.as_bytes()).expect("a link fits in a QR code");
    let width = code.width();
    let modules = code.to_colors();
    let rows = modules.chunks(width).map(|row| {
        div().flex().children(row.iter().map(|m| {
            let dark = *m == qrcode::Color::Dark;
            div().size(px(module)).bg(color(if dark { tokens::QR_DARK } else { tokens::QR_LIGHT }))
        }))
    });
    div()
        .flex_none()
        .p(px(padding))
        .rounded(px(tokens::RADIUS_LARGE))
        .bg(color(tokens::QR_LIGHT))
        .flex()
        .flex_col()
        .children(rows.collect::<Vec<_>>())
}

//! ADR 0005: the look is declared once, in `design/tokens.json`. The Shell's screens draw from the
//! generated constants, so a colour, radius, spacing or type size written past them is a bug.

use m4tv_shell::tokens::{DARK, LIGHT};

/// What `main.rs` may not contain: a colour literal, or gpui's built-in scale for the values the
/// tokens define (padding, gap, radius and text size).
const FORBIDDEN: [&str; 15] = [
    "0x", ".p_", ".px_", ".py_", ".pt_", ".pb_", ".gap_", ".gap_x_", ".rounded_", ".text_xs(",
    ".text_sm(", ".text_lg(", ".text_xl(", ".text_2xl(", ".text_3xl(",
];

#[test]
fn the_shell_draws_from_tokens_and_hard_codes_none_of_them() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).unwrap();
    let found: Vec<_> = source
        .lines()
        .enumerate()
        .flat_map(|(n, line)| {
            let line = line.trim_start();
            let is_code = !line.starts_with("//");
            FORBIDDEN
                .iter()
                .filter(move |bad| is_code && line.contains(**bad))
                .map(move |bad| format!("main.rs:{}: {bad}", n + 1))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_ink_accent_is_white_on_dark_and_near_black_on_light() {
    assert_eq!(DARK.ink, 0xffffff);
    assert_eq!(LIGHT.ink, LIGHT.text);
    assert!(LIGHT.ink < 0x202020);
}

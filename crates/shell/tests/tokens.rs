//! ADR 0005: the look is declared once, in `design/tokens.json`. The Shell's screens draw from the
//! generated constants, so a colour, radius, spacing or type size written past them is a bug.

use std::path::Path;

use m4tv_shell::tokens::{DARK, LIGHT};

/// What `main.rs` and the `ui` modules may not contain: a colour literal, or gpui's built-in scale for the values the
/// tokens define (padding, gap, radius and text size).
const FORBIDDEN: [&str; 18] = [
    "0x", ".p_", ".px_", ".py_", ".pt_", ".pb_", ".gap_", ".gap_x_", ".rounded_sm(", ".rounded_md(", ".rounded_lg(", ".rounded_xl(", ".text_xs(",
    ".text_sm(", ".text_lg(", ".text_xl(", ".text_2xl(", ".text_3xl(",
];

#[test]
fn the_shell_draws_from_tokens_and_hard_codes_none_of_them() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = std::iter::once(src.join("main.rs"))
        .chain(std::fs::read_dir(src.join("ui")).unwrap().map(|e| e.unwrap().path()))
        .filter(|path| path.extension().is_some_and(|e| e == "rs"));
    let mut found = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            let line = line.trim_start();
            if !line.starts_with("//") {
                found.extend(FORBIDDEN.iter().filter(|bad| line.contains(**bad)).map(|bad| format!("{name}:{}: {bad}", n + 1)));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_ink_accent_is_white_on_dark_and_near_black_on_light() {
    assert_eq!(DARK.ink, 0xffffff);
    assert_eq!(LIGHT.ink, LIGHT.text);
    assert!(LIGHT.ink < 0x202020);
}

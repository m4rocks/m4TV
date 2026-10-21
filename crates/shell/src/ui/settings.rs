//! Settings: categories on the left, the selected category's rows on the right.

use gpui::{Div, FontWeight, Pixels, div, prelude::*, px};
use m4tv_shell::{
    shell::{Row, Shell},
    tokens::{self, Palette},
};

use super::color;

pub fn settings(shell: &Shell, p: Palette, margin: Pixels) -> Div {
    let in_rows = shell.settings_in_rows();
    let categories = shell.settings_categories().into_iter().enumerate().map(|(i, category)| {
        let selected = i == shell.settings_category();
        item(p, selected && !in_rows, selected)
            .child(category.label())
    });
    let rows = shell.settings_rows().into_iter().enumerate().map(|(i, row)| {
        let focused = in_rows && i == shell.settings_focused();
        let value = value_of(shell, &row);
        let changes = focused && matches!(row, Row::Setting(_));
        item(p, focused, false)
            .flex()
            .justify_between()
            .child(row.label())
            .child(
                div()
                    .text_color(color(if focused { p.text } else { p.muted }))
                    .child(if changes { format!("‹  {value}  ›") } else { value }),
            )
    });
    div()
        .size_full()
        .flex()
        .flex_col()
        .p(margin)
        .gap(px(tokens::SPACE_6))
        .child(div().text_size(px(tokens::TEXT_3XL)).font_weight(FontWeight::BOLD).child("Settings"))
        .child(
            div()
                .flex_1()
                .flex()
                .gap(px(tokens::SPACE_8))
                .child(
                    div().w(px(280.)).flex().flex_col().gap(px(tokens::SPACE_2)).children(categories),
                )
                .child(
                    div()
                        .flex_1()
                        .max_w(px(720.))
                        .flex()
                        .flex_col()
                        .gap(px(tokens::SPACE_2))
                        .children(rows),
                ),
        )
        .child(
            div()
                .h(px(tokens::SPACE_8))
                .text_size(px(tokens::TEXT_LG))
                .text_color(color(p.state))
                .child(shell.notice().unwrap_or("").to_string()),
        )
}

/// What a row shows on its right: a setting's value, or a hint for the row's action.
fn value_of(shell: &Shell, row: &Row) -> String {
    match row {
        Row::Setting(_) | Row::Version(_) => shell.row_value(row),
        _ => row.hint().to_string(),
    }
}

/// A line of either pane. `focused` draws the thin ink outline; `selected` only raises it.
fn item(p: Palette, focused: bool, selected: bool) -> Div {
    div()
        .px(px(tokens::SPACE_6))
        .py(px(tokens::SPACE_3))
        .rounded(px(tokens::RADIUS_MEDIUM))
        .border_2()
        .border_color(color(if focused { p.ink } else { p.background }))
        .bg(color(if focused || selected { p.raised } else { p.surface }))
        .text_size(px(tokens::TEXT_XL))
}

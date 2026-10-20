//! The Shell. On Windows this is the Dev host: an ordinary window, a keyboard for the TV remote.

use gpui::{
    App, Bounds, Context, FocusHandle, KeyDownEvent, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use gpui_platform::application;
use m4tv_os::FakeOs;
use m4tv_shell::{
    apps::built_in_apps,
    shell::{Key, Row, Screen, Shell},
};

struct ShellView {
    shell: Shell,
    focus: FocusHandle,
}

impl Render for ShellView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        match self.shell.screen().clone() {
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
                        Row::Reboot | Row::PowerOff => String::new(),
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
            // The Engine arrives with the CEF ticket; until then an opened App is a placeholder.
            Screen::App(id) => root
                .child(
                    div().text_3xl().child(
                        self.shell.app(&id).expect("open App is in the catalogue").name.clone(),
                    ),
                )
                .child(div().text_color(rgb(0x8a96a3)).child("Esc or Home to go back")),
        }
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    ShellView {
                        shell: Shell::new(built_in_apps(), Box::new(FakeOs::dev_host())),
                        focus,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

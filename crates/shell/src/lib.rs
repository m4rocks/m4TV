//! The Shell's UI-independent core: App definitions and the TV-remote navigation state.

pub mod apps;
pub mod engine;
pub mod shell;

#[cfg(test)]
mod tests {
    use super::apps::{built_in_apps, parse_catalog};
    use super::engine::Engine;
    use super::shell::{Key, Row, Screen, Shell};
    use m4tv_os::{FakeOs, Os, PowerAction};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fake Os whose settings file is private to the calling test.
    fn fake_os() -> FakeOs {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "m4tv-shell-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        FakeOs::new(dir.join("settings.toml"))
    }

    /// An Engine that records what the Shell asks of it. Clones share one record.
    #[derive(Clone, Default)]
    struct FakeEngine(std::sync::Arc<std::sync::Mutex<Record>>);

    #[derive(Default)]
    struct Record {
        events: Vec<String>,
        history: bool,
    }

    impl FakeEngine {
        fn events(&self) -> Vec<String> {
            self.0.lock().unwrap().events.clone()
        }

        fn set_history(&self, history: bool) {
            self.0.lock().unwrap().history = history;
        }

        fn log(&self, event: String) {
            self.0.lock().unwrap().events.push(event);
        }
    }

    impl Engine for FakeEngine {
        fn open(&mut self, app: &super::apps::AppDefinition) {
            self.log(format!("open {}", app.id));
        }

        fn close(&mut self) {
            self.log("close".into());
        }

        fn key(&mut self, key: Key) {
            self.log(format!("key {key:?}"));
        }

        fn can_go_back(&self) -> bool {
            self.0.lock().unwrap().history
        }

        fn go_back(&mut self) {
            self.log("back".into());
        }

        fn take_frame(&mut self) -> Option<super::engine::Frame> {
            None
        }
    }

    fn shell_with(os: &FakeOs, engine: &FakeEngine) -> Shell {
        Shell::new(built_in_apps(), Box::new(os.clone()), Box::new(engine.clone()))
    }

    fn shell_on(os: &FakeOs) -> Shell {
        shell_with(os, &FakeEngine::default())
    }

    fn shell() -> Shell {
        shell_on(&fake_os())
    }

    fn press(shell: &mut Shell, keys: &[Key]) {
        for key in keys {
            shell.press(*key);
        }
    }

    /// Opens Settings and moves to the row with this label.
    fn at_row(shell: &mut Shell, label: &str) {
        press(shell, &[Key::Right, Key::Right, Key::Right, Key::Enter]);
        let index = shell.settings_rows().iter().position(|row| row.label() == label).unwrap();
        press(shell, &vec![Key::Down; index]);
    }

    #[test]
    fn ships_exactly_three_built_in_apps_in_order() {
        let names: Vec<_> = built_in_apps().into_iter().map(|app| app.name).collect();
        assert_eq!(names, ["YouTube", "Spotify", "Netflix"]);
    }

    #[test]
    fn built_in_apps_have_stable_ids() {
        let ids: Vec<_> = built_in_apps().into_iter().map(|app| app.id).collect();
        assert_eq!(ids, ["youtube", "spotify", "netflix"]);
    }

    fn catalog(ids: &[&str]) -> String {
        ids.iter()
            .map(|id| {
                format!(
                    "[[app]]
id = \"{id}\"
name = \"N\"
icon = \"i\"
start_url = \"https://x\"
"
                )
            })
            .collect()
    }

    #[test]
    fn a_duplicate_id_is_rejected() {
        assert!(parse_catalog(&catalog(&["a", "b"])).is_ok());
        assert!(parse_catalog(&catalog(&["a", "b", "a"])).is_err());
    }

    #[test]
    fn an_id_that_is_not_a_lowercase_slug_is_rejected() {
        for id in ["", "YouTube", "you tube", "-a", "a-", "a--b", "a_b"] {
            assert!(parse_catalog(&catalog(&[id])).is_err(), "{id:?} should be rejected");
        }
        assert!(parse_catalog(&catalog(&["you-tube2"])).is_ok());
    }

    #[test]
    fn a_missing_id_is_rejected() {
        let no_id = "[[app]]
name = \"N\"
icon = \"i\"
start_url = \"https://x\"
";
        assert!(parse_catalog(no_id).is_err());
    }

    #[test]
    fn starts_on_home_with_the_first_tile_focused() {
        let shell = shell();
        assert_eq!(shell.screen(), &Screen::Home);
        assert_eq!(shell.focused(), 0);
    }

    #[test]
    fn right_moves_focus_to_the_next_tile() {
        let mut shell = shell();
        shell.press(Key::Right);
        assert_eq!(shell.focused(), 1);
    }

    #[test]
    fn focus_stops_at_both_ends_instead_of_wrapping() {
        let mut shell = shell();
        shell.press(Key::Left);
        assert_eq!(shell.focused(), 0);
        for _ in 0..5 {
            shell.press(Key::Right);
        }
        assert_eq!(shell.focused(), 3, "the Settings tile follows the three Apps");
        shell.press(Key::Left);
        assert_eq!(shell.focused(), 2);
    }

    #[test]
    fn up_and_down_leave_focus_alone_on_a_single_row() {
        let mut shell = shell();
        shell.press(Key::Right);
        shell.press(Key::Up);
        shell.press(Key::Down);
        assert_eq!(shell.focused(), 1);
    }

    #[test]
    fn enter_opens_the_focused_app() {
        let mut shell = shell();
        shell.press(Key::Right);
        shell.press(Key::Enter);
        assert_eq!(shell.screen(), &Screen::App("spotify".into()));
    }

    #[test]
    fn navigation_keys_do_nothing_while_an_app_is_open() {
        let mut shell = shell();
        shell.press(Key::Enter);
        shell.press(Key::Right);
        shell.press(Key::Enter);
        assert_eq!(shell.screen(), &Screen::App("youtube".into()));
        assert_eq!(shell.focused(), 0);
    }

    #[test]
    fn back_and_home_return_from_an_app_to_the_tile_it_was_opened_from() {
        for key in [Key::Back, Key::Home] {
            let mut shell = shell();
            shell.press(Key::Right);
            shell.press(Key::Enter);
            shell.press(key);
            assert_eq!(shell.screen(), &Screen::Home);
            assert_eq!(shell.focused(), 1);
        }
    }

    #[test]
    fn back_and_home_on_home_do_nothing() {
        let mut shell = shell();
        shell.press(Key::Back);
        shell.press(Key::Home);
        assert_eq!(shell.screen(), &Screen::Home);
        assert_eq!(shell.focused(), 0);
    }

    #[test]
    fn the_dev_host_keyboard_stands_in_for_the_tv_remote() {
        let mapped: Vec<_> = ["up", "down", "left", "right", "enter", "escape", "home", "a"]
            .map(Key::from_keystroke)
            .into();
        assert_eq!(
            mapped,
            [
                Some(Key::Up),
                Some(Key::Down),
                Some(Key::Left),
                Some(Key::Right),
                Some(Key::Enter),
                Some(Key::Back),
                Some(Key::Home),
                None,
            ]
        );
    }

    #[test]
    fn the_settings_tile_opens_settings_and_back_returns_to_it() {
        let mut shell = shell();
        press(&mut shell, &[Key::Right, Key::Right, Key::Right, Key::Enter]);
        assert_eq!(shell.screen(), &Screen::Settings);
        shell.press(Key::Back);
        assert_eq!(shell.screen(), &Screen::Home);
        assert_eq!(shell.focused(), 3);
    }

    #[test]
    fn the_settings_screen_lists_every_declared_setting_then_the_device_actions() {
        let mut shell = shell();
        at_row(&mut shell, "Volume");
        let labels: Vec<_> = shell.settings_rows().iter().map(Row::label).collect();
        assert_eq!(labels, ["Volume", "Mute", "Reboot", "Power off"]);
        assert_eq!(shell.settings_focused(), 0);
    }

    #[test]
    fn settings_focus_moves_up_and_down_and_stops_at_the_ends() {
        let mut shell = shell();
        at_row(&mut shell, "Volume");
        shell.press(Key::Up);
        assert_eq!(shell.settings_focused(), 0);
        press(&mut shell, &[Key::Down; 9]);
        assert_eq!(shell.settings_focused(), 3);
    }

    #[test]
    fn left_and_right_change_a_setting_and_enter_flips_a_toggle() {
        let mut shell = shell();
        at_row(&mut shell, "Volume");
        press(&mut shell, &[Key::Right, Key::Right, Key::Left, Key::Enter]);
        assert_eq!(shell.setting_text("volume"), "55");
        press(&mut shell, &[Key::Down, Key::Enter]);
        assert_eq!(shell.setting_text("mute"), "On");
    }

    #[test]
    fn a_changed_setting_survives_a_restart() {
        let os = fake_os();
        let mut shell = shell_on(&os);
        at_row(&mut shell, "Volume");
        press(&mut shell, &[Key::Left, Key::Left, Key::Down, Key::Enter]);
        drop(shell);
        let restarted = shell_on(&os);
        assert_eq!(restarted.setting_text("volume"), "40");
        assert_eq!(restarted.setting_text("mute"), "On");
    }

    #[test]
    fn reboot_and_power_off_go_through_the_os_seam_and_are_announced() {
        let os = fake_os();
        let mut shell = shell_on(&os);
        at_row(&mut shell, "Reboot");
        shell.press(Key::Enter);
        assert!(shell.notice().unwrap().contains("reboot"));
        press(&mut shell, &[Key::Down, Key::Enter]);
        assert!(shell.notice().unwrap().contains("power off"));
        assert_eq!(os.actions(), [PowerAction::Reboot, PowerAction::PowerOff]);
        shell.press(Key::Up);
        assert_eq!(shell.notice(), None, "the announcement lasts until the next key");
    }

    #[test]
    fn left_and_right_do_nothing_on_a_device_action() {
        let os = fake_os();
        let mut shell = shell_on(&os);
        at_row(&mut shell, "Reboot");
        press(&mut shell, &[Key::Left, Key::Right]);
        assert_eq!(os.actions(), []);
    }

    #[test]
    fn settings_shows_the_version_the_os_reports() {
        let os = fake_os();
        assert_eq!(shell_on(&os).version(), os.version());
    }

    #[test]
    fn opening_an_app_launches_it_in_the_engine() {
        let engine = FakeEngine::default();
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Right, Key::Enter]);
        assert_eq!(engine.events(), ["open spotify"]);
    }

    #[test]
    fn keys_go_to_the_app_while_it_is_open_except_home_and_back() {
        let engine = FakeEngine::default();
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter, Key::Up, Key::Down, Key::Left, Key::Right, Key::Enter]);
        assert_eq!(
            engine.events(),
            ["open youtube", "key Up", "key Down", "key Left", "key Right", "key Enter"]
        );
    }

    #[test]
    fn home_always_returns_to_the_shell_and_closes_the_app() {
        let engine = FakeEngine::default();
        engine.set_history(true);
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter, Key::Home]);
        assert_eq!(shell.screen(), &Screen::Home);
        assert_eq!(engine.events(), ["open youtube", "close"]);
    }

    #[test]
    fn back_goes_to_the_app_first_then_falls_through_to_the_shell() {
        let engine = FakeEngine::default();
        engine.set_history(true);
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter, Key::Back]);
        assert_eq!(shell.screen(), &Screen::App("youtube".into()));
        engine.set_history(false);
        shell.press(Key::Back);
        assert_eq!(shell.screen(), &Screen::Home);
        assert_eq!(engine.events(), ["open youtube", "back", "close"]);
    }

    #[test]
    fn remote_keys_map_to_windows_virtual_key_codes() {
        let codes: Vec<_> = [Key::Left, Key::Up, Key::Right, Key::Down, Key::Enter]
            .map(Key::windows_key_code)
            .into();
        assert_eq!(codes, [Some(0x25), Some(0x26), Some(0x27), Some(0x28), Some(0x0D)]);
        assert_eq!(
            Key::Back.windows_key_code(),
            None,
            "Back and Home are the Shell's, not the page's"
        );
        assert_eq!(Key::Home.windows_key_code(), None);
    }
}

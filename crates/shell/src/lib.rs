//! The Shell's UI-independent core: App definitions and the TV-remote navigation state.

pub mod apps;
pub mod electron_engine;
pub mod engine;
pub mod remote;
pub mod shell;

/// The design tokens of ADR 0005 (`design/tokens.json`), turned into constants by build.rs.
pub mod tokens {
    include!(concat!(env!("OUT_DIR"), "/tokens.rs"));
}

#[cfg(test)]
mod tests {
    use super::apps::{built_in_apps, page_script, parse_catalog};
    use super::engine::{Cdm, Engine, Session};
    use super::shell::{Key, OVERLAY_TIME, Overlay, Row, Screen, Shell, Theme};
    use std::time::Duration;
    use m4tv_os::{FakeOs, Network, Os, PowerAction, WallClock};
    use m4tv_settings::Category;
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
        cdm: Option<Cdm>,
    }

    impl FakeEngine {
        fn events(&self) -> Vec<String> {
            self.0.lock().unwrap().events.clone()
        }

        fn set_cdm(&self, cdm: Cdm) {
            self.0.lock().unwrap().cdm = Some(cdm);
        }

        fn set_history(&self, history: bool) {
            self.0.lock().unwrap().history = history;
        }

        fn log(&self, event: String) {
            self.0.lock().unwrap().events.push(event);
        }
    }

    impl Engine for FakeEngine {
        fn cdm(&self) -> Cdm {
            self.0.lock().unwrap().cdm.clone().unwrap_or(Cdm::Ready)
        }

        fn retry_cdm(&mut self) {
            self.log("retry cdm".into());
            self.0.lock().unwrap().cdm = Some(Cdm::Preparing);
        }

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

        fn editable_focused(&self) -> bool {
            false
        }

        fn type_text(&mut self, _text: &str) {}
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
    /// Opens Settings and moves to the row called `label`: its category on the left, then the row.
    fn at_row(shell: &mut Shell, label: &str) {
        press(shell, &[Key::Right, Key::Right, Key::Right, Key::Enter]);
        let category = shell
            .all_settings_rows()
            .iter()
            .find(|row| row.label() == label)
            .map(Row::category)
            .unwrap();
        let category = shell.settings_categories().iter().position(|c| *c == category).unwrap();
        press(shell, &vec![Key::Down; category]);
        shell.press(Key::Enter);
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

    #[test]
    fn youtube_runs_youtube_tv_with_a_tv_user_agent_and_the_others_use_the_default() {
        let apps = built_in_apps();
        assert_eq!(apps[0].start_url, "https://www.youtube.com/tv");
        assert!(apps[0].user_agent.as_deref().unwrap().contains("PlayStation"));
        assert_eq!(apps[1].user_agent, None);
        assert_eq!(apps[2].user_agent, None);
    }

    #[test]
    fn every_app_gets_spatial_navigation_until_it_turns_it_off() {
        let mut app = built_in_apps().remove(0);
        assert!(page_script(&app).contains("m4tvSpatialNav"));
        app.tv.spatial_nav = false;
        assert!(!page_script(&app).contains("m4tvSpatialNav"));
    }

    #[test]
    fn an_apps_own_css_and_js_are_injected_with_the_script() {
        let mut app = built_in_apps().remove(0);
        app.css = Some("body{zoom:1.5}".into());
        app.js = Some("window.tuned=1;".into());
        let script = page_script(&app);
        assert!(script.contains("body{zoom:1.5}") && script.contains("window.tuned=1;"));
    }

    #[test]
    fn css_with_quotes_and_newlines_stays_inside_its_js_string() {
        let mut app = built_in_apps().remove(0);
        app.tv.spatial_nav = false;
        app.css = Some("a::after{content:'x\\'}\n".into());
        assert!(page_script(&app).contains(r"'a::after{content:\'x\\\'}\n'"));
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
        at_row(&mut shell, "Theme");
        let labels: Vec<_> = shell.all_settings_rows().iter().map(Row::label).collect();
        assert_eq!(
            labels,
            [
                "Theme",
                "Reduce motion",
                "24-hour clock",
                "Volume",
                "Mute",
                "Version",
                "Reboot",
                "Power off"
            ]
        );
        assert_eq!(shell.settings_focused(), 0);
    }

    #[test]
    fn settings_focus_moves_up_and_down_and_stops_at_the_ends() {
        let mut shell = shell();
        at_row(&mut shell, "Theme");
        shell.press(Key::Up);
        assert_eq!(shell.settings_focused(), 0);
        press(&mut shell, &[Key::Down; 12]);
        assert_eq!(shell.settings_focused(), 2, "Display has three rows");
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
    fn the_display_settings_change_from_the_keys_and_a_choice_wraps() {
        let mut shell = shell();
        at_row(&mut shell, "Theme");
        assert_eq!(shell.setting_text("theme"), "Dark");
        press(&mut shell, &[Key::Right]);
        assert_eq!(shell.setting_text("theme"), "Light");
        press(&mut shell, &[Key::Right]);
        assert_eq!(shell.setting_text("theme"), "Dark");
        press(&mut shell, &[Key::Down, Key::Enter, Key::Down, Key::Enter]);
        assert_eq!(shell.setting_text("reduce_motion"), "On");
        assert_eq!(shell.setting_text("clock_24h"), "Off");
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
    fn settings_has_two_panes_and_back_steps_out_one_at_a_time() {
        let mut shell = shell();
        press(&mut shell, &[Key::Right, Key::Right, Key::Right, Key::Enter]);
        assert_eq!(shell.screen(), &Screen::Settings);
        // No Remote is attached, so there is nothing to list under Network.
        assert_eq!(
            shell.settings_categories(),
            [Category::Display, Category::Sound, Category::System]
        );
        assert!(!shell.settings_in_rows());

        // Up and Down move the categories; the rows on the right follow.
        press(&mut shell, &[Key::Up, Key::Down, Key::Down]);
        assert_eq!(shell.settings_category(), 2);
        assert_eq!(shell.settings_rows().iter().map(Row::label).collect::<Vec<_>>(), [
            "Version", "Reboot", "Power off"
        ]);
        press(&mut shell, &[Key::Down]);
        assert_eq!(shell.settings_category(), 2, "the last category is the end");

        // Left and Right on a category change nothing; Right or Enter enters the rows.
        press(&mut shell, &[Key::Up, Key::Left]);
        assert!(!shell.settings_in_rows());
        shell.press(Key::Right);
        assert!(shell.settings_in_rows());
        press(&mut shell, &[Key::Right, Key::Right]);
        assert_eq!(shell.setting_text("volume"), "60", "Right changes the value in the rows");

        // Back goes to the categories, a second Back leaves Settings.
        shell.press(Key::Back);
        assert_eq!((shell.screen(), shell.settings_in_rows()), (&Screen::Settings, false));
        shell.press(Key::Back);
        assert_eq!(shell.screen(), &Screen::Home);

        // Settings opens on the first category again.
        press(&mut shell, &[Key::Enter]);
        assert_eq!((shell.settings_category(), shell.settings_in_rows()), (0, false));
    }

    #[test]
    fn home_leaves_settings_from_either_pane() {
        let mut shell = shell();
        at_row(&mut shell, "Volume");
        shell.press(Key::Home);
        assert_eq!(shell.screen(), &Screen::Home);
        press(&mut shell, &[Key::Enter, Key::Home]);
        assert_eq!(shell.screen(), &Screen::Home);
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
    fn the_hero_shows_the_clock_only_once_the_time_is_known_in_the_chosen_format() {
        let os = fake_os();
        let mut shell = shell_on(&os);
        assert_eq!(shell.clock_text(), None, "a Pi does not know the time at first");
        assert!(!shell.tick(Duration::ZERO), "nothing changed yet");

        os.set_clock(Some(WallClock { hour: 15, minute: 4 }));
        assert!(shell.tick(Duration::ZERO), "the clock appearing is a change");
        assert_eq!(shell.clock_text().as_deref(), Some("15:04"));
        assert!(!shell.tick(Duration::ZERO), "an idle Home redraws only when the clock changes");
        os.set_clock(Some(WallClock { hour: 15, minute: 5 }));
        assert!(shell.tick(Duration::ZERO));

        at_row(&mut shell, "24-hour clock");
        shell.press(Key::Enter);
        assert_eq!(shell.clock_text().as_deref(), Some("3:05 PM"));
        os.set_clock(Some(WallClock { hour: 0, minute: 30 }));
        assert_eq!(shell.clock_text().as_deref(), Some("12:30 AM"));
        os.set_clock(Some(WallClock { hour: 12, minute: 0 }));
        assert_eq!(shell.clock_text().as_deref(), Some("12:00 PM"));
    }

    #[test]
    fn the_hotspot_takeover_shows_a_failure_but_not_a_join_under_way() {
        let hotspot = |status: &str| Overlay::Hotspot {
            ssid: "m4TV".into(),
            password: "pw".into(),
            payload: String::new(),
            url: String::new(),
            status: status.into(),
        };
        assert_eq!(hotspot("").hotspot_failure(), None);
        assert_eq!(hotspot("Joining Home...").hotspot_failure(), None);
        assert_eq!(hotspot("wrong password").hotspot_failure(), Some("wrong password"));
        assert_eq!(Overlay::Volume { level: 1, muted: false }.hotspot_failure(), None);
    }

    #[test]
    fn the_hero_status_names_the_network() {
        let os = fake_os();
        let shell = shell_on(&os);
        assert_eq!(shell.status_line(), "Wi-Fi 192.168.1.20");
        os.set_network(Network::Ethernet);
        assert_eq!(shell.status_line(), "Ethernet");
        os.set_network(Network::None);
        assert_eq!(shell.status_line(), "No network");
    }

    #[test]
    fn the_theme_and_reduce_motion_settings_reach_the_screens() {
        let mut shell = shell();
        assert_eq!((shell.theme(), shell.reduce_motion()), (Theme::Dark, false));
        at_row(&mut shell, "Theme");
        shell.press(Key::Right);
        assert_eq!(shell.theme(), Theme::Light);
        press(&mut shell, &[Key::Down, Key::Enter]);
        assert!(shell.reduce_motion());
    }

    #[test]
    fn an_app_asked_for_while_video_playback_prepares_waits_behind_an_overlay() {
        let engine = FakeEngine::default();
        engine.set_cdm(Cdm::Preparing);
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter]);
        assert_eq!(shell.screen(), &Screen::Home);
        assert_eq!(shell.overlay(), Some(&Overlay::Cdm { reason: None }));
        assert_eq!(engine.events(), [] as [String; 0], "nothing opened yet");

        // It continues into the App by itself once the Engine has it.
        engine.set_cdm(Cdm::Ready);
        shell.tick(Duration::ZERO);
        assert_eq!(shell.screen(), &Screen::App("youtube".into()));
        assert_eq!(engine.events(), ["open youtube"]);
        assert_eq!(shell.overlay(), None);
    }

    #[test]
    fn a_volume_change_shows_over_the_cdm_overlay_and_the_overlay_returns() {
        let engine = FakeEngine::default();
        engine.set_cdm(Cdm::Preparing);
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter, Key::VolumeUp]);
        assert!(matches!(shell.overlay(), Some(Overlay::Volume { .. })));
        shell.tick(Duration::ZERO);
        assert!(matches!(shell.overlay(), Some(Overlay::Volume { .. })), "not overwritten");
        shell.tick(OVERLAY_TIME);
        assert_eq!(shell.overlay(), Some(&Overlay::Cdm { reason: None }));
    }

    #[test]
    fn a_failed_cdm_says_why_retry_starts_again_and_back_gives_up() {
        let engine = FakeEngine::default();
        engine.set_cdm(Cdm::Failed("offline: the TV has no internet connection".into()));
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter]);
        let reason = Some("offline: the TV has no internet connection".to_string());
        assert_eq!(shell.overlay(), Some(&Overlay::Cdm { reason }));

        shell.press(Key::Enter); // Retry
        assert_eq!(engine.events(), ["retry cdm"]);
        shell.tick(Duration::ZERO);
        assert_eq!(shell.overlay(), Some(&Overlay::Cdm { reason: None }), "preparing again");

        engine.set_cdm(Cdm::Failed("blocked: the download was refused".into()));
        shell.tick(Duration::ZERO);
        shell.press(Key::Back);
        assert_eq!(shell.overlay(), None);
        engine.set_cdm(Cdm::Ready);
        shell.tick(Duration::ZERO);
        assert_eq!(shell.screen(), &Screen::Home, "Back cancelled the launch");
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
    fn the_dev_host_keyboard_has_volume_keys() {
        let mapped: Vec<_> = ["pageup", "pagedown", "m"].map(Key::from_keystroke).into();
        assert_eq!(mapped, [Some(Key::VolumeUp), Some(Key::VolumeDown), Some(Key::Mute)]);
    }

    fn volume(level: i64, muted: bool) -> Option<Overlay> {
        Some(Overlay::Volume { level, muted })
    }

    #[test]
    fn a_volume_key_shows_an_overlay_over_a_playing_app_without_touching_it() {
        let engine = FakeEngine::default();
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter, Key::VolumeUp]);
        assert_eq!(shell.overlay(), volume(55, false).as_ref());
        assert_eq!(shell.screen(), &Screen::App("youtube".into()));
        assert_eq!(engine.events(), ["open youtube"]);
    }

    #[test]
    fn keys_still_reach_the_app_while_an_overlay_is_visible() {
        let engine = FakeEngine::default();
        let mut shell = shell_with(&fake_os(), &engine);
        press(&mut shell, &[Key::Enter, Key::VolumeDown, Key::Right, Key::Enter]);
        assert!(shell.overlay().is_some());
        assert_eq!(engine.events(), ["open youtube", "key Right", "key Enter"]);
    }

    #[test]
    fn mute_toggles_and_the_overlay_says_so() {
        let mut shell = shell();
        shell.press(Key::Mute);
        assert_eq!(shell.overlay(), volume(50, true).as_ref());
        shell.press(Key::Mute);
        assert_eq!(shell.overlay(), volume(50, false).as_ref());
    }

    #[test]
    fn volume_keys_change_the_stored_volume_from_any_screen() {
        let os = fake_os();
        let mut shell = shell_on(&os);
        press(&mut shell, &[Key::VolumeDown, Key::VolumeDown]);
        assert_eq!(shell_on(&os).setting_text("volume"), "40");
    }

    #[test]
    fn the_overlay_dismisses_itself_and_a_new_press_restarts_the_wait() {
        let mut shell = shell();
        shell.press(Key::VolumeUp);
        shell.tick(OVERLAY_TIME / 2);
        shell.press(Key::VolumeUp);
        shell.tick(OVERLAY_TIME / 2);
        assert_eq!(shell.overlay(), volume(60, false).as_ref(), "the second press restarted it");
        shell.tick(OVERLAY_TIME / 2);
        assert_eq!(shell.overlay(), None);
    }

    #[test]
    fn an_overlay_can_be_dismissed_on_request() {
        let mut shell = shell();
        shell.press(Key::VolumeUp);
        shell.dismiss_overlay();
        assert_eq!(shell.overlay(), None);
    }

    #[test]
    fn a_dismissed_overlay_stays_dismissed_after_the_app_closes() {
        let mut shell = shell();
        press(&mut shell, &[Key::Enter, Key::VolumeUp, Key::Home]);
        shell.tick(OVERLAY_TIME);
        assert_eq!(shell.overlay(), None);
    }

    #[test]
    fn remote_keys_map_to_the_names_electron_takes_for_input_events() {
        let codes: Vec<_> = [Key::Left, Key::Up, Key::Right, Key::Down, Key::Enter]
            .map(Key::electron_key_code)
            .into();
        assert_eq!(
            codes,
            [Some("Left"), Some("Up"), Some("Right"), Some("Down"), Some("Return")]
        );
        assert_eq!(
            Key::Back.electron_key_code(),
            None,
            "Back and Home are the Shell's, not the page's"
        );
        assert_eq!(Key::Home.electron_key_code(), None);
    }

    fn frame() -> super::engine::Frame {
        super::engine::Frame { width: 1, height: 1, bgra: vec![0; 4] }
    }

    #[test]
    fn a_browser_created_in_the_open_session_is_kept() {
        let mut session = Session::default();
        let (id, stale) = session.begin();
        assert_eq!(stale, None);
        assert_eq!(session.attach(id, "page"), None);
        assert_eq!(session.browser(), Some(&"page"));
    }

    #[test]
    fn a_browser_that_arrives_after_its_session_ended_is_handed_back_to_close() {
        let mut session = Session::default();
        let (id, _) = session.begin();
        assert_eq!(session.end(), None, "nothing was created yet, so nothing to close");
        assert_eq!(session.attach(id, "late"), Some("late"));
        assert_eq!(session.browser(), None);
    }

    #[test]
    fn a_frame_painted_after_the_session_ended_is_dropped() {
        let mut session = Session::<&str>::default();
        let (id, _) = session.begin();
        session.paint(id, frame());
        session.end();
        session.paint(id, frame());
        assert!(session.take_frame().is_none());
    }

    #[test]
    fn ending_returns_the_browser_and_clears_its_frame() {
        let mut session = Session::default();
        let (id, _) = session.begin();
        session.attach(id, "page");
        session.paint(id, frame());
        assert_eq!(session.end(), Some("page"));
        assert!(session.take_frame().is_none());
    }

    #[test]
    fn callbacks_from_an_earlier_session_cannot_touch_the_next_one() {
        let mut session = Session::default();
        let (first, _) = session.begin();
        session.end();
        let (second, _) = session.begin();
        session.paint(first, frame());
        assert!(session.take_frame().is_none());
        assert_eq!(session.attach(first, "old"), Some("old"));
        assert_eq!(session.attach(second, "new"), None);
        assert_eq!(session.browser(), Some(&"new"));
    }

    #[test]
    fn beginning_again_hands_back_a_browser_that_was_never_ended() {
        let mut session = Session::default();
        let (first, _) = session.begin();
        session.attach(first, "page");
        let (_, stale) = session.begin();
        assert_eq!(stale, Some("page"));
    }

    #[test]
    fn an_editable_field_is_tracked_for_the_open_session_only() {
        let mut session = Session::<&str>::default();
        let (first, _) = session.begin();
        session.set_editable(first, true);
        assert!(session.editable());
        session.end();
        assert!(!session.editable(), "closing the App clears it");
        session.set_editable(first, true);
        assert!(!session.editable(), "a late callback from the old session is refused");
    }

    #[test]
    fn remote_key_codes_map_to_keys() {
        let cases = [
            ("ArrowLeft", Key::Left),
            ("ArrowUp", Key::Up),
            ("Enter", Key::Enter),
            ("Escape", Key::Back),
            ("Home", Key::Home),
            ("AudioVolumeMute", Key::Mute),
            ("MediaPlayPause", Key::PlayPause),
            ("MediaTrackNext", Key::NextTrack),
            ("MediaTrackPrevious", Key::PreviousTrack),
        ];
        for (code, key) in cases {
            assert_eq!(Key::from_code(code), Some(key), "{code}");
        }
        assert_eq!(Key::from_code("KeyA"), None);
        // Media keys reach the page; Back, Home and volume stay with the Shell.
        assert_eq!(Key::PlayPause.electron_key_code(), Some("MediaPlayPause"));
        assert_eq!(Key::Home.electron_key_code(), None);
        // What the Remote's buttons send ends up as an Electron input event.
        let sent: Vec<_> = ["ArrowLeft", "Enter", "MediaTrackNext", "Escape"]
            .map(|code| Key::from_code(code).and_then(Key::electron_key_code))
            .into();
        assert_eq!(sent, [Some("Left"), Some("Return"), Some("MediaNextTrack"), None]);
    }
}

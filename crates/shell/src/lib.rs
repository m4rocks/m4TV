//! The Shell's UI-independent core: App definitions and the TV-remote navigation state.

pub mod apps;
pub mod shell;

#[cfg(test)]
mod tests {
    use super::apps::built_in_apps;
    use super::shell::{Key, Screen, Shell};

    fn shell() -> Shell {
        Shell::new(built_in_apps())
    }

    #[test]
    fn ships_exactly_three_built_in_apps_in_order() {
        let names: Vec<_> = built_in_apps().into_iter().map(|app| app.name).collect();
        assert_eq!(names, ["YouTube", "Spotify", "Netflix"]);
    }

    #[test]
    fn starts_on_home_with_the_first_tile_focused() {
        let shell = shell();
        assert_eq!(shell.screen(), Screen::Home);
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
        assert_eq!(shell.focused(), 2);
        shell.press(Key::Left);
        assert_eq!(shell.focused(), 1);
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
        assert_eq!(shell.screen(), Screen::App(1));
    }

    #[test]
    fn navigation_keys_do_nothing_while_an_app_is_open() {
        let mut shell = shell();
        shell.press(Key::Enter);
        shell.press(Key::Right);
        shell.press(Key::Enter);
        assert_eq!(shell.screen(), Screen::App(0));
        assert_eq!(shell.focused(), 0);
    }

    #[test]
    fn back_and_home_return_from_an_app_to_the_tile_it_was_opened_from() {
        for key in [Key::Back, Key::Home] {
            let mut shell = shell();
            shell.press(Key::Right);
            shell.press(Key::Enter);
            shell.press(key);
            assert_eq!(shell.screen(), Screen::Home);
            assert_eq!(shell.focused(), 1);
        }
    }

    #[test]
    fn back_and_home_on_home_do_nothing() {
        let mut shell = shell();
        shell.press(Key::Back);
        shell.press(Key::Home);
        assert_eq!(shell.screen(), Screen::Home);
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
}

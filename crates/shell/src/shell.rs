//! The Shell's navigation state, driven by TV-remote keys and observable without any UI.

use crate::apps::AppDefinition;

/// A TV-remote key. The dev-host keyboard and, later, the Remote both produce these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Back,
    Home,
}

impl Key {
    /// Maps a gpui key name, the Dev host's stand-in for the TV remote: arrows, Enter,
    /// Esc as Back, and the Home key as Home.
    pub fn from_keystroke(key: &str) -> Option<Key> {
        Some(match key {
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "enter" => Key::Enter,
            "escape" => Key::Back,
            "home" => Key::Home,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    /// The App with this id is open.
    App(String),
}

pub struct Shell {
    apps: Vec<AppDefinition>,
    screen: Screen,
    focused: usize,
}

impl Shell {
    pub fn new(apps: Vec<AppDefinition>) -> Self {
        Self { apps, screen: Screen::Home, focused: 0 }
    }

    pub fn apps(&self) -> &[AppDefinition] {
        &self.apps
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn app(&self, id: &str) -> Option<&AppDefinition> {
        self.apps.iter().find(|app| app.id == id)
    }

    /// Index of the focused tile on Home.
    pub fn focused(&self) -> usize {
        self.focused
    }

    pub fn press(&mut self, key: Key) {
        // While an App is open its keys belong to the App (wired up with the Engine), except
        // Home, which the Shell reserves. Back falls through to Home until the Engine can take it first.
        if self.screen != Screen::Home {
            if matches!(key, Key::Back | Key::Home) {
                self.screen = Screen::Home;
            }
            return;
        }
        match key {
            Key::Left => self.focused = self.focused.saturating_sub(1),
            Key::Right => self.focused = (self.focused + 1).min(self.apps.len().saturating_sub(1)),
            Key::Enter => {
                if let Some(app) = self.apps.get(self.focused) {
                    self.screen = Screen::App(app.id.clone());
                }
            }
            _ => {}
        }
    }
}

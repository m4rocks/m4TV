//! The Shell's navigation state, driven by TV-remote keys and observable without any UI.

use m4tv_os::Os;
use m4tv_settings::{Kind, SETTINGS, SettingDef, Settings};

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
    Settings,
}

/// A line on the Settings screen: every declared setting, then the device actions.
#[derive(Clone, Copy)]
pub enum Row {
    Setting(&'static SettingDef),
    Reboot,
    PowerOff,
}

impl Row {
    pub fn label(&self) -> &'static str {
        match self {
            Row::Setting(def) => def.label,
            Row::Reboot => "Reboot",
            Row::PowerOff => "Power off",
        }
    }
}

pub struct Shell {
    apps: Vec<AppDefinition>,
    os: Box<dyn Os>,
    settings: Settings,
    screen: Screen,
    focused: usize,
    settings_focused: usize,
    notice: Option<String>,
}

impl Shell {
    pub fn new(apps: Vec<AppDefinition>, os: Box<dyn Os>) -> Self {
        let settings = Settings::load(&os.settings_path());
        Self {
            apps,
            os,
            settings,
            screen: Screen::Home,
            focused: 0,
            settings_focused: 0,
            notice: None,
        }
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

    /// Index of the focused tile on Home. The Settings tile follows the App tiles.
    pub fn focused(&self) -> usize {
        self.focused
    }

    pub fn settings_rows(&self) -> Vec<Row> {
        SETTINGS.iter().map(Row::Setting).chain([Row::Reboot, Row::PowerOff]).collect()
    }

    pub fn settings_focused(&self) -> usize {
        self.settings_focused
    }

    /// The displayed value of a declared setting.
    pub fn setting_text(&self, key: &str) -> String {
        self.settings.get(key).to_string()
    }

    /// What the last device action or failed save said, until the next key.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn version(&self) -> String {
        self.os.version()
    }

    pub fn press(&mut self, key: Key) {
        match self.screen {
            Screen::Home => self.press_home(key),
            // While an App is open its keys belong to the App (wired up with the Engine), except
            // Home, which the Shell reserves. Back falls through to Home until the Engine can take it first.
            Screen::App(_) => {
                if matches!(key, Key::Back | Key::Home) {
                    self.screen = Screen::Home;
                }
            }
            Screen::Settings => self.press_settings(key),
        }
    }

    fn press_home(&mut self, key: Key) {
        match key {
            Key::Left => self.focused = self.focused.saturating_sub(1),
            Key::Right => self.focused = (self.focused + 1).min(self.apps.len()),
            Key::Enter => {
                self.screen = match self.apps.get(self.focused) {
                    Some(app) => Screen::App(app.id.clone()),
                    None => {
                        self.settings_focused = 0;
                        Screen::Settings
                    }
                }
            }
            _ => {}
        }
    }

    fn press_settings(&mut self, key: Key) {
        self.notice = None;
        let rows = self.settings_rows();
        match key {
            Key::Back | Key::Home => self.screen = Screen::Home,
            Key::Up => self.settings_focused = self.settings_focused.saturating_sub(1),
            Key::Down => self.settings_focused = (self.settings_focused + 1).min(rows.len() - 1),
            Key::Left | Key::Right | Key::Enter => match rows[self.settings_focused] {
                Row::Setting(def) if key != Key::Enter || matches!(def.kind, Kind::Toggle) => {
                    self.settings.adjust(def.key, if key == Key::Left { -1 } else { 1 });
                    if let Err(e) = self.settings.save(&self.os.settings_path()) {
                        self.notice = Some(format!("Could not save settings: {e}"));
                    }
                }
                Row::Reboot if key == Key::Enter => self.notice = Some(self.os.reboot()),
                Row::PowerOff if key == Key::Enter => self.notice = Some(self.os.power_off()),
                _ => {}
            },
        }
    }
}

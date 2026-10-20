//! The Shell's navigation state, driven by TV-remote keys and observable without any UI.

use std::time::Duration;

use m4tv_os::Os;
use m4tv_settings::{Kind, SETTINGS, SettingDef, Settings, Value};

use crate::{
    apps::AppDefinition,
    engine::{Engine, Frame},
};

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
    VolumeUp,
    VolumeDown,
    Mute,
}

impl Key {
    /// The Windows virtual-key code a page sees for this key. Back, Home and the volume keys
    /// belong to the Shell.
    pub fn windows_key_code(self) -> Option<i32> {
        Some(match self {
            Key::Left => 0x25,
            Key::Up => 0x26,
            Key::Right => 0x27,
            Key::Down => 0x28,
            Key::Enter => 0x0D,
            Key::Back | Key::Home | Key::VolumeUp | Key::VolumeDown | Key::Mute => return None,
        })
    }

    /// Maps a gpui key name, the Dev host's stand-in for the TV remote: arrows, Enter,
    /// Esc as Back, the Home key as Home, and PageUp, PageDown and M for volume.
    pub fn from_keystroke(key: &str) -> Option<Key> {
        Some(match key {
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "enter" => Key::Enter,
            "escape" => Key::Back,
            "home" => Key::Home,
            "pageup" => Key::VolumeUp,
            "pagedown" => Key::VolumeDown,
            "m" => Key::Mute,
            _ => return None,
        })
    }
}

/// How long an Overlay stays after the last thing that raised it.
pub const OVERLAY_TIME: Duration = Duration::from_secs(2);

/// Shell interface drawn over whatever screen is up, an App included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    Volume { level: i64, muted: bool },
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
    engine: Box<dyn Engine>,
    settings: Settings,
    screen: Screen,
    focused: usize,
    settings_focused: usize,
    notice: Option<String>,
    /// The Overlay showing, and how long it has left.
    overlay: Option<(Overlay, Duration)>,
}

impl Shell {
    pub fn new(apps: Vec<AppDefinition>, os: Box<dyn Os>, engine: Box<dyn Engine>) -> Self {
        let settings = Settings::load(&os.settings_path());
        Self {
            apps,
            os,
            engine,
            settings,
            screen: Screen::Home,
            focused: 0,
            settings_focused: 0,
            notice: None,
            overlay: None,
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

    /// The Overlay to draw over the current screen, if one is showing.
    pub fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref().map(|(overlay, _)| overlay)
    }

    pub fn dismiss_overlay(&mut self) {
        self.overlay = None;
    }

    /// Lets time pass: an Overlay dismisses itself once its wait is over.
    pub fn tick(&mut self, elapsed: Duration) {
        if let Some((_, left)) = &mut self.overlay {
            *left = left.saturating_sub(elapsed);
            if left.is_zero() {
                self.overlay = None;
            }
        }
    }

    pub fn version(&self) -> String {
        self.os.version()
    }

    pub fn press(&mut self, key: Key) {
        // Volume is the Shell's on every screen, so an open App never sees these keys.
        if let Key::VolumeUp | Key::VolumeDown | Key::Mute = key {
            return self.press_volume(key);
        }
        match self.screen {
            Screen::Home => self.press_home(key),
            Screen::App(_) => self.press_app(key),
            Screen::Settings => self.press_settings(key),
        }
    }

    /// While an App is open its keys belong to the App, except Home, which the Shell reserves.
    /// Back goes to the App first and falls through to the Shell when it has nothing to go back to.
    fn press_app(&mut self, key: Key) {
        match key {
            Key::Home => self.close_app(),
            Key::Back if !self.engine.can_go_back() => self.close_app(),
            Key::Back => self.engine.go_back(),
            _ => self.engine.key(key),
        }
    }

    fn press_volume(&mut self, key: Key) {
        match key {
            Key::VolumeUp => self.adjust("volume", 1),
            Key::VolumeDown => self.adjust("volume", -1),
            _ => self.adjust("mute", 1),
        }
        let number = |key| match self.settings.get(key) {
            Value::Int(n) => n,
            Value::Bool(on) => on as i64,
        };
        let overlay = Overlay::Volume { level: number("volume"), muted: number("mute") != 0 };
        self.overlay = Some((overlay, OVERLAY_TIME));
    }

    /// Steps a setting and stores it, saying so if it could not be stored.
    fn adjust(&mut self, key: &str, direction: i64) {
        self.settings.adjust(key, direction);
        if let Err(e) = self.settings.save(&self.os.settings_path()) {
            self.notice = Some(format!("Could not save settings: {e}"));
        }
    }

    fn close_app(&mut self) {
        self.engine.close();
        self.screen = Screen::Home;
    }

    /// The newest frame of the open App.
    pub fn take_frame(&mut self) -> Option<Frame> {
        self.engine.take_frame()
    }

    fn press_home(&mut self, key: Key) {
        match key {
            Key::Left => self.focused = self.focused.saturating_sub(1),
            Key::Right => self.focused = (self.focused + 1).min(self.apps.len()),
            Key::Enter => {
                self.screen = match self.apps.get(self.focused) {
                    Some(app) => {
                        self.engine.open(app);
                        Screen::App(app.id.clone())
                    }
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
            Key::VolumeUp | Key::VolumeDown | Key::Mute => {} // handled in press
            Key::Left | Key::Right | Key::Enter => match rows[self.settings_focused] {
                Row::Setting(def) if key != Key::Enter || matches!(def.kind, Kind::Toggle) => {
                    self.adjust(def.key, if key == Key::Left { -1 } else { 1 });
                }
                Row::Reboot if key == Key::Enter => self.notice = Some(self.os.reboot()),
                Row::PowerOff if key == Key::Enter => self.notice = Some(self.os.power_off()),
                _ => {}
            },
        }
    }
}

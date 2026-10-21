//! The Shell's navigation state, driven by TV-remote keys and observable without any UI.

use std::time::Duration;

use m4tv_os::Os;
use m4tv_settings::{Kind, SETTINGS, SettingDef, Settings, Value};

use crate::{
    apps::AppDefinition,
    engine::{Engine, Frame},
    remote::{AppSummary, Command, LiveState, Phone, Remote, SettingKind, SettingState},
};

mod setup;
use setup::{Setup, setup_overlay};

/// A TV-remote key. The Dev host keyboard and the Remote both produce these.
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
    PlayPause,
    NextTrack,
    PreviousTrack,
}

impl Key {
    /// The Windows virtual-key code a page sees for this key. Back, Home and the volume keys
    /// belong to the Shell; media keys go to the page.
    pub fn windows_key_code(self) -> Option<i32> {
        Some(match self {
            Key::Left => 0x25,
            Key::Up => 0x26,
            Key::Right => 0x27,
            Key::Down => 0x28,
            Key::Enter => 0x0D,
            Key::PlayPause => 0xB3,
            Key::NextTrack => 0xB0,
            Key::PreviousTrack => 0xB1,
            Key::Back | Key::Home | Key::VolumeUp | Key::VolumeDown | Key::Mute => return None,
        })
    }

    /// Maps a browser `KeyboardEvent.code`, which is what the Remote sends for its d-pad and
    /// media buttons.
    pub fn from_code(code: &str) -> Option<Key> {
        Some(match code {
            "ArrowUp" => Key::Up,
            "ArrowDown" => Key::Down,
            "ArrowLeft" => Key::Left,
            "ArrowRight" => Key::Right,
            "Enter" | "NumpadEnter" => Key::Enter,
            "Escape" | "BrowserBack" => Key::Back,
            "Home" | "BrowserHome" => Key::Home,
            "AudioVolumeUp" => Key::VolumeUp,
            "AudioVolumeDown" => Key::VolumeDown,
            "AudioVolumeMute" => Key::Mute,
            "MediaPlayPause" => Key::PlayPause,
            "MediaTrackNext" => Key::NextTrack,
            "MediaTrackPrevious" => Key::PreviousTrack,
            _ => return None,
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
    Volume {
        level: i64,
        muted: bool,
    },
    /// How to pair a phone: scan the QR code `payload`, or type `pin`. `cert_replaced` says the
    /// TV's certificate changed, so phones that were paired must pair again.
    Pairing {
        pin: String,
        payload: String,
        cert_replaced: bool,
    },
    /// The Setup hotspot: scan `payload` (a Wi-Fi QR code) or type `password` to join `ssid`,
    /// then open the Remote at `url`. `status` says how joining a Wi-Fi is going, or why it failed.
    Hotspot {
        ssid: String,
        password: String,
        payload: String,
        url: String,
        status: String,
    },
    /// The TV joined a Wi-Fi: scan `payload` to pair at its new `address`.
    Joined {
        address: String,
        payload: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    /// The App with this id is open.
    App(String),
    Settings,
}

/// A line on the Settings screen: every declared setting, the Remote's phones, then the device
/// actions.
#[derive(Clone)]
pub enum Row {
    Setting(&'static SettingDef),
    /// Shows the Pairing Overlay.
    Pair,
    /// Starts the Setup hotspot, or stops one started here.
    Hotspot,
    /// A paired phone; Enter unpairs it.
    Phone(Phone),
    Reboot,
    PowerOff,
}

impl Row {
    pub fn label(&self) -> String {
        match self {
            Row::Setting(def) => def.label.into(),
            Row::Pair => "Pair a phone".into(),
            Row::Hotspot => "Setup hotspot".into(),
            Row::Phone(phone) => phone.name.clone(),
            Row::Reboot => "Reboot".into(),
            Row::PowerOff => "Power off".into(),
        }
    }

    /// What the row shows beside its label when it is not a setting's value.
    pub fn hint(&self) -> &'static str {
        match self {
            Row::Phone(_) => "Enter to unpair",
            Row::Hotspot => "Enter to start or stop",
            _ => "",
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
    /// The Overlay showing, and how long it has left. A Pairing Overlay has no time limit.
    overlay: Option<(Overlay, Duration)>,
    remote: Option<Remote>,
    /// Settings asked for the Pairing Overlay, which then stays until Back or a Pairing.
    pairing_requested: bool,
    /// How many phones were paired when the Shell last looked.
    phones_seen: usize,
    setup: Setup,
    /// The Settings row for the on-demand hotspot warned that it disconnects the TV, and the very
    /// next key decides whether it starts.
    hotspot_warned: bool,
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
            remote: None,
            pairing_requested: false,
            phones_seen: 0,
            setup: Setup::Off,
            hotspot_warned: false,
        }
    }

    /// Starts taking commands from phones.
    pub fn attach_remote(&mut self, remote: Remote) {
        self.phones_seen = remote.pairings().phones().len();
        self.remote = Some(remote);
        self.sync_remote();
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
        let phones = self.remote.iter().flat_map(|r| r.pairings().phones());
        SETTINGS
            .iter()
            .map(Row::Setting)
            .chain(self.remote.iter().map(|_| Row::Pair))
            .chain(self.remote.iter().map(|_| Row::Hotspot))
            .chain(phones.map(Row::Phone))
            .chain([Row::Reboot, Row::PowerOff])
            .collect()
    }

    pub fn settings_focused(&self) -> usize {
        self.settings_focused
    }

    /// The displayed value of a declared setting.
    pub fn setting_text(&self, key: &str) -> String {
        self.settings.text(key)
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

    /// Lets time pass: an Overlay dismisses itself once its wait is over, and phones get their
    /// turn. Returns whether anything on screen may have changed.
    pub fn tick(&mut self, elapsed: Duration) -> bool {
        let before = self.overlay.clone();
        if let Some((overlay, left)) = &mut self.overlay
            && matches!(overlay, Overlay::Volume { .. })
        {
            *left = left.saturating_sub(elapsed);
            if left.is_zero() {
                self.overlay = None;
            }
        }
        let commanded = self.run_remote_commands();
        self.drive_setup(elapsed);
        self.sync_remote();
        commanded || self.overlay != before
    }

    fn run_remote_commands(&mut self) -> bool {
        let mut ran = false;
        while let Some(command) = self.remote.as_mut().and_then(Remote::next_command) {
            ran = true;
            match command {
                Command::Key(key) => self.press(key),
                Command::Launch(id) => self.launch(&id),
                Command::Order(ids) => self.reorder(&ids),
                Command::Setting(key, value) => self.set(&key, value),
                Command::Step(key, direction) => {
                    self.adjust(&key, direction);
                    if matches!(key.as_str(), "volume" | "mute") {
                        self.show_volume();
                    }
                }
                Command::Join(request) => self.join(request),
                Command::Text(text) => {
                    if self.editable_focused() {
                        self.engine.type_text(&text);
                    }
                }
            }
        }
        ran
    }

    fn editable_focused(&self) -> bool {
        matches!(self.screen, Screen::App(_)) && self.engine.editable_focused()
    }

    /// Opens an App by id, closing the one that is open.
    fn launch(&mut self, id: &str) {
        let Some(app) = self.apps.iter().find(|app| app.id == id) else {
            return;
        };
        if matches!(self.screen, Screen::App(_)) {
            self.engine.close();
        }
        self.engine.open(app);
        self.screen = Screen::App(app.id.clone());
    }

    /// Puts the Apps in the order of `ids`, keeping the focus on the same tile.
    fn reorder(&mut self, ids: &[String]) {
        let focused = self.apps.get(self.focused).map(|app| app.id.clone());
        self.apps.sort_by_key(|app| ids.iter().position(|id| *id == app.id));
        if let Some(id) = focused {
            self.focused = self.apps.iter().position(|app| app.id == id).unwrap_or(0);
        }
    }

    /// Keeps the Pairing Overlay, the phone count and the Remote's view of the Shell current.
    fn sync_remote(&mut self) {
        let Some(phones) = self.remote.as_ref().map(|remote| remote.pairings().phones().len())
        else {
            return;
        };
        if phones > self.phones_seen {
            self.pairing_requested = false;
            if matches!(self.setup, Setup::Joined { .. }) {
                self.end_handover();
            }
        }
        self.phones_seen = phones;
        let Some(remote) = &self.remote else { return };
        if let Some(overlay) = setup_overlay(&self.setup, remote) {
            self.overlay = Some((overlay, Duration::MAX));
            remote.publish(self.live_state());
            return;
        }
        if matches!(self.overlay, Some((Overlay::Hotspot { .. } | Overlay::Joined { .. }, _))) {
            self.overlay = None;
        }
        // Nobody has the Remote open: show how to pair, except over a playing App.
        let wanted = self.pairing_requested
            || (remote.connections() == 0 && !matches!(self.screen, Screen::App(_)));
        if wanted {
            let (pin, secret) = remote.pairings().offer();
            let overlay = Overlay::Pairing {
                payload: remote.pairing_link(&secret),
                cert_replaced: remote.cert_replaced() && phones == 0,
                pin,
            };
            self.overlay = Some((overlay, Duration::MAX));
        } else if matches!(self.overlay, Some((Overlay::Pairing { .. }, _))) {
            self.overlay = None;
            remote.pairings().cancel();
        }
        remote.publish(self.live_state());
    }

    fn live_state(&self) -> LiveState {
        let (screen, app, focus) = match &self.screen {
            Screen::Home => ("home", None, self.focused),
            Screen::App(id) => ("app", Some(id.clone()), 0),
            Screen::Settings => ("settings", None, self.settings_focused),
        };
        LiveState {
            screen,
            app,
            focus,
            apps: self
                .apps
                .iter()
                .map(|app| AppSummary { id: app.id.clone(), name: app.name.clone() })
                .collect(),
            volume: self.number("volume"),
            muted: self.number("mute") != 0,
            editable: self.editable_focused(),
            setup: self.setup.view(),
            notice: self.notice.clone(),
            settings: SETTINGS
                .iter()
                .map(|def| SettingState {
                    key: def.key,
                    label: def.label,
                    category: def.category.label(),
                    kind: match def.kind {
                        Kind::Range { min, max, step } => SettingKind::Range { min, max, step },
                        Kind::Toggle => SettingKind::Toggle,
                        Kind::Choice { options } => {
                            SettingKind::Choice { options: options.to_vec() }
                        }
                    },
                    value: self.settings.get(def.key),
                })
                .collect(),
        }
    }

    /// A setting as a number, a toggle being 0 or 1.
    fn number(&self, key: &str) -> i64 {
        match self.settings.get(key) {
            Value::Int(n) => n,
            Value::Bool(on) => on as i64,
        }
    }

    pub fn version(&self) -> String {
        self.os.version()
    }

    pub fn press(&mut self, key: Key) {
        self.dispatch(key);
        self.sync_remote();
    }

    fn dispatch(&mut self, key: Key) {
        // Volume is the Shell's on every screen, so an open App never sees these keys.
        if let Key::VolumeUp | Key::VolumeDown | Key::Mute = key {
            return self.press_volume(key);
        }
        if key == Key::Back && self.pairing_requested {
            self.pairing_requested = false;
            return;
        }
        if key == Key::Back && self.put_setup_away() {
            return;
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
        self.show_volume();
    }

    fn show_volume(&mut self) {
        // Pairing and setup are how a phone gets in, so a volume change does not cover them.
        if matches!(self.overlay, None | Some((Overlay::Volume { .. }, _))) {
            let overlay =
                Overlay::Volume { level: self.number("volume"), muted: self.number("mute") != 0 };
            self.overlay = Some((overlay, OVERLAY_TIME));
        }
    }

    /// Steps a setting and stores it, saying so if it could not be stored.
    fn adjust(&mut self, key: &str, direction: i64) {
        self.settings.adjust(key, direction);
        self.store();
    }

    /// Sets a setting a phone asked for and stores it.
    fn set(&mut self, key: &str, value: Value) {
        self.settings.set(key, value);
        self.store();
        if matches!(key, "volume" | "mute") {
            self.show_volume();
        }
    }

    fn store(&mut self) {
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

    fn revoke(&mut self, id: u64) {
        let Some(remote) = &self.remote else { return };
        if let Err(e) = remote.pairings().revoke(id) {
            self.notice = Some(format!("Could not save paired phones: {e}"));
        }
        let rows = self.settings_rows().len();
        self.settings_focused = self.settings_focused.min(rows - 1);
    }

    fn press_settings(&mut self, key: Key) {
        self.notice = None;
        let warned = std::mem::take(&mut self.hotspot_warned);
        let rows = self.settings_rows();
        match key {
            Key::Back | Key::Home => self.screen = Screen::Home,
            Key::Up => self.settings_focused = self.settings_focused.saturating_sub(1),
            Key::Down => self.settings_focused = (self.settings_focused + 1).min(rows.len() - 1),
            Key::VolumeUp | Key::VolumeDown | Key::Mute => {} // handled in press
            Key::PlayPause | Key::NextTrack | Key::PreviousTrack => {}
            Key::Left | Key::Right | Key::Enter => match rows[self.settings_focused].clone() {
                Row::Setting(def) if key != Key::Enter || matches!(def.kind, Kind::Toggle) => {
                    self.adjust(def.key, if key == Key::Left { -1 } else { 1 });
                }
                Row::Pair if key == Key::Enter => self.pairing_requested = true,
                Row::Hotspot if key == Key::Enter => self.ask_for_hotspot(warned),
                Row::Phone(phone) if key == Key::Enter => self.revoke(phone.id),
                Row::Reboot if key == Key::Enter => self.notice = Some(self.os.reboot()),
                Row::PowerOff if key == Key::Enter => self.notice = Some(self.os.power_off()),
                _ => {}
            },
        }
    }
}

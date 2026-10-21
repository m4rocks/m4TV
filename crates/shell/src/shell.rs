//! The Shell's navigation state, driven by TV-remote keys and observable without any UI.

use std::time::Duration;

use m4tv_os::{Network, Os, WallClock};
use m4tv_settings::{Category, Kind, SETTINGS, SettingDef, Settings, Value};

use crate::{
    apps::AppDefinition,
    engine::{Cdm, Engine, Frame},
    remote::{
        AppSummary, CdmView, Command, LiveState, Phone, Remote, SettingKind, SettingState,
    },
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
    /// The name Electron's `sendInputEvent` takes for this key. Back, Home and the volume keys
    /// belong to the Shell; media keys go to the page.
    pub fn electron_key_code(self) -> Option<&'static str> {
        Some(match self {
            Key::Left => "Left",
            Key::Up => "Up",
            Key::Right => "Right",
            Key::Down => "Down",
            Key::Enter => "Return",
            Key::PlayPause => "MediaPlayPause",
            Key::NextTrack => "MediaNextTrack",
            Key::PreviousTrack => "MediaPreviousTrack",
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

/// The Shell's appearance, a Setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
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
    /// An App was asked for while the Engine's video playback is not ready: `reason` is why it
    /// could not be prepared, if it could not.
    Cdm {
        reason: Option<String>,
    },
}

impl Overlay {
    /// Why the Setup hotspot is back or could not join a Wi-Fi: the Overlay's status, unless it is
    /// only saying that a join is under way.
    pub fn hotspot_joining(&self) -> Option<&str> {
        match self {
            Overlay::Hotspot { status, .. } if status.starts_with("Joining") => Some(status),
            _ => None,
        }
    }

    pub fn hotspot_failure(&self) -> Option<&str> {
        match self {
            Overlay::Hotspot { status, .. } if !status.is_empty() && self.hotspot_joining().is_none() => {
                Some(status)
            }
            _ => None,
        }
    }
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
    /// The version of the Shell, shown and not changed.
    Version(String),
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
            Row::Version(_) => "Version".into(),
            Row::Reboot => "Reboot".into(),
            Row::PowerOff => "Power off".into(),
        }
    }

    /// The category of the Settings screen this row is listed under.
    pub fn category(&self) -> Category {
        match self {
            Row::Setting(def) => def.category,
            Row::Pair | Row::Hotspot | Row::Phone(_) => Category::Network,
            Row::Version(_) | Row::Reboot | Row::PowerOff => Category::System,
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
    /// The selected category on the Settings screen, and whether the focus is in its rows.
    settings_category: usize,
    settings_in_rows: bool,
    /// The row focused in the selected category.
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
    /// An App was asked for while the Engine's video playback was not ready: it opens when it is.
    waiting_for_app: Option<String>,
    /// The clock as the Hero last showed it, so an idle Home redraws only when it changes.
    clock_shown: Option<String>,
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
            settings_category: 0,
            settings_in_rows: false,
            settings_focused: 0,
            notice: None,
            overlay: None,
            remote: None,
            pairing_requested: false,
            phones_seen: 0,
            setup: Setup::Off,
            hotspot_warned: false,
            waiting_for_app: None,
            clock_shown: None,
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

    /// Every row of the Settings screen, in category order.
    pub fn all_settings_rows(&self) -> Vec<Row> {
        let phones = self.remote.iter().flat_map(|r| r.pairings().phones());
        let mut rows: Vec<Row> = SETTINGS
            .iter()
            .map(Row::Setting)
            .chain(self.remote.iter().map(|_| Row::Pair))
            .chain(self.remote.iter().map(|_| Row::Hotspot))
            .chain(phones.map(Row::Phone))
            .chain([Row::Version(self.os.version()), Row::Reboot, Row::PowerOff])
            .collect();
        rows.sort_by_key(|row| Category::ALL.iter().position(|c| *c == row.category()));
        rows
    }

    /// The categories on the Settings screen's left pane: the ones that have rows.
    pub fn settings_categories(&self) -> Vec<Category> {
        let rows = self.all_settings_rows();
        Category::ALL.into_iter().filter(|c| rows.iter().any(|r| r.category() == *c)).collect()
    }

    /// The rows of the selected category: the Settings screen's right pane.
    pub fn settings_rows(&self) -> Vec<Row> {
        let category = self.settings_categories().get(self.settings_category).copied();
        self.all_settings_rows().into_iter().filter(|r| Some(r.category()) == category).collect()
    }

    /// The index of the selected category.
    pub fn settings_category(&self) -> usize {
        self.settings_category
    }

    /// Whether the focus is in the rows (the right pane) and not on the categories.
    pub fn settings_in_rows(&self) -> bool {
        self.settings_in_rows
    }

    /// The focused row of the selected category.
    pub fn settings_focused(&self) -> usize {
        self.settings_focused
    }

    /// The displayed value of a declared setting.
    pub fn setting_text(&self, key: &str) -> String {
        self.settings.text(key)
    }

    /// What a row shows beside its label.
    pub fn row_value(&self, row: &Row) -> String {
        match row {
            Row::Setting(def) => self.setting_text(def.key),
            Row::Version(version) => version.clone(),
            _ => row.hint().into(),
        }
    }

    pub fn theme(&self) -> Theme {
        if self.number("theme") == 1 { Theme::Light } else { Theme::Dark }
    }

    pub fn reduce_motion(&self) -> bool {
        self.number("reduce_motion") != 0
    }

    /// The time for the Hero, in the format the Setting asks for. `None` until the device knows
    /// the time (a Pi has no battery-backed clock), so a wrong time is never shown.
    pub fn clock_text(&self) -> Option<String> {
        let WallClock { hour, minute } = self.os.wall_clock()?;
        Some(if self.number("clock_24h") != 0 {
            format!("{hour:02}:{minute:02}")
        } else {
            let (twelve, half) = (if hour % 12 == 0 { 12 } else { hour % 12 }, ["AM", "PM"]);
            format!("{twelve}:{minute:02} {}", half[(hour >= 12) as usize])
        })
    }

    /// The Hero's one-line status: the network, and how many phones are paired.
    pub fn status_line(&self) -> String {
        let network = match self.os.network() {
            Network::None => "No network".to_string(),
            Network::Ethernet => "Ethernet".to_string(),
            Network::Wifi { address } => format!("Wi-Fi {address}"),
        };
        let Some(remote) = &self.remote else { return network };
        match remote.pairings().phones().len() {
            0 => format!("{network} · No phone paired"),
            1 => format!("{network} · 1 phone paired"),
            n => format!("{network} · {n} phones paired"),
        }
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
        self.poll_cdm();
        self.sync_remote();
        let clock = self.clock_text();
        let clock_changed = clock != self.clock_shown;
        self.clock_shown = clock;
        commanded || clock_changed || self.overlay != before
    }

    /// Opens the App that was waiting for video playback once the Engine has it, and keeps the
    /// Overlay that says why the App has not opened.
    fn poll_cdm(&mut self) {
        if let Some(id) = self.waiting_for_app.clone()
            && self.engine.cdm() == Cdm::Ready
        {
            self.waiting_for_app = None;
            self.open_app(&id);
        }
        let waiting = self.waiting_for_app.is_some();
        if !waiting {
            if matches!(self.overlay, Some((Overlay::Cdm { .. }, _))) {
                self.overlay = None;
            }
            return;
        }
        // Setting the TV up comes first; the Overlay comes back when that is over.
        if matches!(self.overlay, Some((Overlay::Hotspot { .. } | Overlay::Joined { .. }, _))) {
            return;
        }
        let reason = match self.engine.cdm() {
            Cdm::Failed(reason) => Some(reason),
            _ => None,
        };
        // A volume change shows for its moment; the Overlay comes back after it.
        if matches!(self.overlay, None | Some((Overlay::Cdm { .. }, _))) {
            self.overlay = Some((Overlay::Cdm { reason }, Duration::MAX));
        }
    }

    fn run_remote_commands(&mut self) -> bool {
        let mut ran = false;
        while let Some(command) = self.remote.as_mut().and_then(Remote::next_command) {
            ran = true;
            match command {
                Command::Key(key) => self.press(key),
                Command::Launch(id) => self.open_app(&id),
                Command::RetryCdm => self.engine.retry_cdm(),
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

    /// Opens an App by id, closing the one that is open. While the Engine is still preparing
    /// video playback the App waits behind an Overlay and opens when it is ready.
    fn open_app(&mut self, id: &str) {
        let Some(app) = self.apps.iter().find(|app| app.id == id) else {
            return;
        };
        if self.engine.cdm() != Cdm::Ready {
            self.waiting_for_app = Some(app.id.clone());
            self.poll_cdm();
            return;
        }
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
        // Nobody has the Remote open: show how to pair, except over a playing App, or while an App
        // waits for video playback.
        let wanted = self.pairing_requested
            || (remote.connections() == 0 && !matches!(self.screen, Screen::App(_)));
        let cdm_up = matches!(self.overlay, Some((Overlay::Cdm { .. }, _)));
        if wanted && !cdm_up {
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
            cdm: match self.engine.cdm() {
                Cdm::Preparing => CdmView::Preparing,
                Cdm::Ready => CdmView::Ready,
                Cdm::Failed(reason) => CdmView::Failed { reason },
            },
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
        if self.waiting_for_app.is_some() {
            match key {
                Key::Back => {
                    self.waiting_for_app = None;
                    return self.poll_cdm();
                }
                Key::Enter => {
                    if matches!(self.engine.cdm(), Cdm::Failed(_)) {
                        self.engine.retry_cdm();
                    }
                    return;
                }
                _ => {}
            }
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
        if matches!(self.overlay, None | Some((Overlay::Volume { .. } | Overlay::Cdm { .. }, _))) {
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
            Key::Enter => match self.apps.get(self.focused) {
                Some(app) => {
                    let id = app.id.clone();
                    self.open_app(&id);
                }
                None => {
                    self.settings_category = 0;
                    self.settings_in_rows = false;
                    self.settings_focused = 0;
                    self.screen = Screen::Settings;
                }
            },
            _ => {}
        }
    }

    fn revoke(&mut self, id: u64) {
        let Some(remote) = &self.remote else { return };
        if let Err(e) = remote.pairings().revoke(id) {
            self.notice = Some(format!("Could not save paired phones: {e}"));
        }
        let rows = self.settings_rows().len();
        self.settings_focused = self.settings_focused.min(rows.saturating_sub(1));
    }

    fn press_settings(&mut self, key: Key) {
        self.notice = None;
        let warned = std::mem::take(&mut self.hotspot_warned);
        if !self.settings_in_rows {
            let last = self.settings_categories().len().saturating_sub(1);
            self.settings_category = self.settings_category.min(last);
            match key {
                Key::Back | Key::Home => self.screen = Screen::Home,
                Key::Up => self.settings_category = self.settings_category.saturating_sub(1),
                Key::Down => self.settings_category = (self.settings_category + 1).min(last),
                Key::Enter | Key::Right => {
                    self.settings_in_rows = true;
                    self.settings_focused = 0;
                }
                _ => {}
            }
            return;
        }
        let rows = self.settings_rows();
        // The category may have gone with its last row (a phone's Remote, say).
        if rows.is_empty() {
            self.settings_in_rows = false;
            return;
        }
        match key {
            Key::Back => self.settings_in_rows = false,
            Key::Home => self.screen = Screen::Home,
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
                Row::Version(_) => {}
                Row::Reboot if key == Key::Enter => self.notice = Some(self.os.reboot()),
                Row::PowerOff if key == Key::Enter => self.notice = Some(self.os.power_off()),
                _ => {}
            },
        }
    }
}

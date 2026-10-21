//! The OS seam (ADR 0003): everything specific to the device (Setup hotspot and Wi-Fi join, audio,
//! power and reboot, CDM fetch location, settings paths) lives behind this crate, with a real
//! implementation for the Pi and a fake for the Dev host. The Shell never calls device APIs itself.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Reboot,
    PowerOff,
}

/// What the device is connected to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    None,
    Ethernet,
    Wifi { address: String },
}

/// The Setup hotspot the TV broadcasts: the Shell picks the name and password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    pub ssid: String,
    pub password: String,
}

/// WPA2 and WPA3 personal, or an open network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Open,
    Wpa2,
    Wpa3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub security: Security,
}

/// A Wi-Fi for the TV to join. `hidden` ones are not in the scan and are asked for by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRequest {
    pub ssid: String,
    pub password: Option<String>,
    pub security: Security,
    pub hidden: bool,
}

/// How the last join is going. Joining is slow, so the Shell asks again each tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinStatus {
    Idle,
    Joining,
    Joined { address: String },
    Failed(String),
}

/// Everything the Shell needs from the device. Power calls return the message the Shell shows.
pub trait Os {
    fn settings_path(&self) -> PathBuf;
    fn version(&self) -> String;
    fn reboot(&self) -> String;
    fn power_off(&self) -> String;
    fn network(&self) -> Network;
    fn start_hotspot(&self, hotspot: &Hotspot);
    fn stop_hotspot(&self);
    /// The visible Wi-Fi networks, hidden ones not included.
    fn scan_wifi(&self) -> Vec<WifiNetwork>;
    /// Starts joining. Never blocks: the outcome shows up in `join_status`.
    fn join_wifi(&self, request: &JoinRequest);
    fn join_status(&self) -> JoinStatus;
}

/// The Dev host's stand-in: power actions are recorded and announced, never carried out.
/// Clones share one log, so a test can keep a handle after handing the Os to the Shell.
#[derive(Debug, Clone)]
pub struct FakeOs {
    settings_path: PathBuf,
    log: Arc<Mutex<Vec<PowerAction>>>,
    net: Arc<Mutex<FakeNet>>,
}

/// A Wi-Fi the fake knows about, and what it takes to join it.
#[derive(Debug)]
struct FakeWifi {
    network: WifiNetwork,
    password: Option<String>,
    hidden: bool,
    address: String,
}

#[derive(Debug)]
struct FakeNet {
    network: Network,
    hotspot: Option<Hotspot>,
    wifi: Vec<FakeWifi>,
    /// The router never hands out an address, so a join never finishes.
    stall: bool,
    join: JoinStatus,
}

impl Default for FakeNet {
    /// Connected, so nothing but a test that scripts otherwise ever sees the hotspot.
    fn default() -> Self {
        Self {
            network: Network::Wifi { address: "192.168.1.20".into() },
            hotspot: None,
            wifi: Vec::new(),
            stall: false,
            join: JoinStatus::Idle,
        }
    }
}

impl FakeOs {
    pub fn new(settings_path: PathBuf) -> Self {
        Self { settings_path, log: Arc::default(), net: Arc::default() }
    }

    /// Keeps the Dev host's settings in the working directory, which git ignores.
    pub fn dev_host() -> Self {
        Self::new(PathBuf::from(".m4tv-dev/settings.toml"))
    }

    pub fn actions(&self) -> Vec<PowerAction> {
        self.log.lock().unwrap().clone()
    }

    /// Scripts what the device is connected to.
    pub fn set_network(&self, network: Network) {
        self.net.lock().unwrap().network = network;
    }

    /// Adds a Wi-Fi that joins with `password` and gets `address`.
    pub fn add_wifi(
        &self,
        ssid: &str,
        security: Security,
        password: Option<&str>,
        hidden: bool,
        address: &str,
    ) {
        self.net.lock().unwrap().wifi.push(FakeWifi {
            network: WifiNetwork { ssid: ssid.into(), security },
            password: password.map(Into::into),
            hidden,
            address: address.into(),
        });
    }

    /// Makes joins hang, as a router that never answers DHCP does.
    pub fn stall_dhcp(&self, stall: bool) {
        self.net.lock().unwrap().stall = stall;
    }

    /// The hotspot being broadcast, if any.
    pub fn hotspot(&self) -> Option<Hotspot> {
        self.net.lock().unwrap().hotspot.clone()
    }

    fn record(&self, action: PowerAction) {
        self.log.lock().unwrap().push(action);
    }
}

impl Os for FakeOs {
    fn settings_path(&self) -> PathBuf {
        self.settings_path.clone()
    }

    fn version(&self) -> String {
        format!("{} (Dev host)", env!("CARGO_PKG_VERSION"))
    }

    fn reboot(&self) -> String {
        self.record(PowerAction::Reboot);
        "Dev host: reboot requested, nothing restarted".into()
    }

    fn power_off(&self) -> String {
        self.record(PowerAction::PowerOff);
        "Dev host: power off requested, nothing shut down".into()
    }

    fn network(&self) -> Network {
        self.net.lock().unwrap().network.clone()
    }

    fn start_hotspot(&self, hotspot: &Hotspot) {
        let mut net = self.net.lock().unwrap();
        net.hotspot = Some(hotspot.clone());
        net.join = JoinStatus::Idle;
    }

    fn stop_hotspot(&self) {
        self.net.lock().unwrap().hotspot = None;
    }

    fn scan_wifi(&self) -> Vec<WifiNetwork> {
        let net = self.net.lock().unwrap();
        net.wifi.iter().filter(|w| !w.hidden).map(|w| w.network.clone()).collect()
    }

    fn join_wifi(&self, request: &JoinRequest) {
        let mut net = self.net.lock().unwrap();
        if net.stall {
            net.join = JoinStatus::Joining;
            return;
        }
        let found = net
            .wifi
            .iter()
            .find(|w| w.network.ssid == request.ssid && (!w.hidden || request.hidden));
        net.join = match found {
            None => JoinStatus::Failed("that network was not found".into()),
            Some(w) if w.network.security != request.security => {
                JoinStatus::Failed("the network uses a different security type".into())
            }
            Some(w) if w.password != request.password => {
                JoinStatus::Failed("wrong password".into())
            }
            Some(w) => {
                let address = w.address.clone();
                net.network = Network::Wifi { address: address.clone() };
                JoinStatus::Joined { address }
            }
        };
    }

    fn join_status(&self) -> JoinStatus {
        self.net.lock().unwrap().join.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_records_and_announces_power_actions_without_acting() {
        let os = FakeOs::new("s.toml".into());
        let handle = os.clone();
        assert!(os.reboot().contains("reboot"));
        assert!(os.power_off().contains("power off"));
        assert_eq!(handle.actions(), [PowerAction::Reboot, PowerAction::PowerOff]);
    }

    fn request(
        ssid: &str,
        password: Option<&str>,
        security: Security,
        hidden: bool,
    ) -> JoinRequest {
        JoinRequest { ssid: ssid.into(), password: password.map(Into::into), security, hidden }
    }

    #[test]
    fn the_fake_joins_the_networks_it_knows_and_says_why_it_cannot_join_others() {
        let os = FakeOs::new("s.toml".into());
        os.add_wifi("Home", Security::Wpa2, Some("hunter22"), false, "10.0.0.7");
        os.add_wifi("Secret", Security::Wpa3, Some("hunter22"), true, "10.0.0.8");
        assert_eq!(os.scan_wifi(), [WifiNetwork { ssid: "Home".into(), security: Security::Wpa2 }]);

        os.join_wifi(&request("Home", Some("nope"), Security::Wpa2, false));
        assert_eq!(os.join_status(), JoinStatus::Failed("wrong password".into()));
        os.join_wifi(&request("Secret", Some("hunter22"), Security::Wpa3, false));
        assert!(matches!(os.join_status(), JoinStatus::Failed(_)), "hidden ones must be asked for");
        os.join_wifi(&request("Secret", Some("hunter22"), Security::Wpa3, true));
        assert_eq!(os.join_status(), JoinStatus::Joined { address: "10.0.0.8".into() });
        assert_eq!(os.network(), Network::Wifi { address: "10.0.0.8".into() });
    }

    #[test]
    fn a_stalled_router_leaves_the_join_pending() {
        let os = FakeOs::new("s.toml".into());
        os.add_wifi("Home", Security::Open, None, false, "10.0.0.7");
        os.stall_dhcp(true);
        os.join_wifi(&request("Home", None, Security::Open, false));
        assert_eq!(os.join_status(), JoinStatus::Joining);
    }

    #[test]
    fn the_fake_reports_its_settings_path_and_a_dev_host_version() {
        let os = FakeOs::new("dir/s.toml".into());
        assert_eq!(os.settings_path(), PathBuf::from("dir/s.toml"));
        assert!(os.version().ends_with("(Dev host)"));
    }
}

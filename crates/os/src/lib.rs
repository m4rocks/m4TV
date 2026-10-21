//! The OS seam (ADR 0003): everything specific to the device (Setup hotspot and Wi-Fi join, audio,
//! power and reboot, CDM fetch location, settings paths) lives behind this crate, with a real
//! implementation for the Pi and a fake for the Dev host. The Shell never calls device APIs itself.

use std::{
    net::Ipv4Addr,
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
    /// Starts broadcasting and returns the address a phone on the hotspot reaches the TV at.
    fn start_hotspot(&self, hotspot: &Hotspot) -> Ipv4Addr;
    fn stop_hotspot(&self);
    /// The visible Wi-Fi networks, hidden ones not included.
    fn scan_wifi(&self) -> Vec<WifiNetwork>;
    /// Starts joining. Never blocks: the outcome shows up in `join_status`.
    fn join_wifi(&self, request: &JoinRequest);
    fn join_status(&self) -> JoinStatus;
    /// Gives up on a join that has not finished, so it cannot finish later.
    fn cancel_join(&self);
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
    /// The router is not answering DHCP: a join waits in `pending` until it does.
    stall: bool,
    pending: Option<JoinRequest>,
    join: JoinStatus,
}

impl FakeNet {
    /// Works out how a join to `request` goes.
    fn finish_join(&mut self, request: &JoinRequest) {
        let found = self
            .wifi
            .iter()
            .find(|w| w.network.ssid == request.ssid && (!w.hidden || request.hidden));
        self.join = match found {
            None => JoinStatus::Failed("that network was not found".into()),
            Some(w) if w.network.security != request.security => {
                JoinStatus::Failed("the network uses a different security type".into())
            }
            Some(w) if w.password != request.password => {
                JoinStatus::Failed("wrong password".into())
            }
            Some(w) => {
                let address = w.address.clone();
                self.network = Network::Wifi { address: address.clone() };
                JoinStatus::Joined { address }
            }
        };
    }
}

impl Default for FakeNet {
    /// Connected, so nothing but a test that scripts otherwise ever sees the hotspot.
    fn default() -> Self {
        Self {
            network: Network::Wifi { address: "192.168.1.20".into() },
            hotspot: None,
            wifi: Vec::new(),
            stall: false,
            pending: None,
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

    /// Makes joins wait, as a router that is slow to answer DHCP does. Turning it off lets a join
    /// that is still waiting finish.
    pub fn stall_dhcp(&self, stall: bool) {
        let mut net = self.net.lock().unwrap();
        net.stall = stall;
        if !stall && let Some(request) = net.pending.take() {
            net.finish_join(&request);
        }
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

    fn start_hotspot(&self, hotspot: &Hotspot) -> Ipv4Addr {
        let mut net = self.net.lock().unwrap();
        net.hotspot = Some(hotspot.clone());
        net.join = JoinStatus::Idle;
        Ipv4Addr::LOCALHOST
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
            net.pending = Some(request.clone());
            net.join = JoinStatus::Joining;
        } else {
            net.finish_join(request);
        }
    }

    fn join_status(&self) -> JoinStatus {
        self.net.lock().unwrap().join.clone()
    }

    fn cancel_join(&self) {
        let mut net = self.net.lock().unwrap();
        net.pending = None;
        net.join = JoinStatus::Idle;
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
    fn a_cancelled_join_does_not_finish_when_the_router_answers_late() {
        let os = FakeOs::new("s.toml".into());
        os.add_wifi("Home", Security::Open, None, false, "10.0.0.7");
        os.stall_dhcp(true);
        os.join_wifi(&request("Home", None, Security::Open, false));
        os.cancel_join();
        os.stall_dhcp(false);
        assert_eq!(os.join_status(), JoinStatus::Idle);
        assert_eq!(os.network(), Network::Wifi { address: "192.168.1.20".into() });

        os.stall_dhcp(true);
        os.join_wifi(&request("Home", None, Security::Open, false));
        os.stall_dhcp(false);
        assert_eq!(os.join_status(), JoinStatus::Joined { address: "10.0.0.7".into() });
    }

    #[test]
    fn the_fake_reports_its_settings_path_and_a_dev_host_version() {
        let os = FakeOs::new("dir/s.toml".into());
        assert_eq!(os.settings_path(), PathBuf::from("dir/s.toml"));
        assert!(os.version().ends_with("(Dev host)"));
    }
}

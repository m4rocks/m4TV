//! The Setup hotspot flow: the hotspot comes up when the TV has no network (or is asked for), a
//! phone on it picks a Wi-Fi, and the TV hands over to its new address. It is a child of
//! `shell` so it can drive the Shell's own state without that state being public.

use std::time::Duration;

use m4tv_os::{Hotspot, JoinStatus, Network, Security, WifiNetwork};

use super::{Overlay, Shell};
use crate::remote::{Remote, SetupView, WifiView};

/// The name and password of a new hotspot. People type the password, so it avoids look-alikes.
fn new_hotspot() -> Hotspot {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).expect("the OS has randomness");
    // 256 is a multiple of the alphabet's 32 letters, so the modulo is unbiased.
    let pick = |bytes: &[u8]| -> String {
        bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
    };
    Hotspot { ssid: format!("m4TV-Setup-{}", pick(&random[..4])), password: pick(&random[4..]) }
}

/// The text of a Wi-Fi QR code: `WIFI:T:WPA;S:<name>;P:<password>;;`, with its special characters
/// escaped.
fn wifi_qr(hotspot: &Hotspot) -> String {
    let escape = |text: &str| {
        text.chars().fold(String::new(), |mut out, c| {
            if "\\;,:\"".contains(c) {
                out.push('\\');
            }
            out.push(c);
            out
        })
    };
    format!("WIFI:T:WPA;S:{};P:{};;", escape(&hotspot.ssid), escape(&hotspot.password))
}

fn security_name(security: Security) -> &'static str {
    match security {
        Security::Open => "open",
        Security::Wpa2 => "wpa2",
        Security::Wpa3 => "wpa3",
    }
}

/// The Overlay the Setup flow puts up, if it is running.
pub(super) fn setup_overlay(setup: &Setup, remote: &Remote) -> Option<Overlay> {
    let hotspot_overlay = |session: &Session, status: String| Overlay::Hotspot {
        ssid: session.hotspot.ssid.clone(),
        password: session.hotspot.password.clone(),
        payload: wifi_qr(&session.hotspot),
        url: session.url.clone(),
        status,
    };
    match setup {
        Setup::Off => None,
        // Waiting for a Wi-Fi with a phone already on the Remote: the panel has done its job, as
        // the Pairing one does. A failure or a join in progress still shows.
        Setup::Hotspot { reason: None, .. } if remote.connections() > 0 => None,
        Setup::Hotspot { session, reason, .. } => {
            Some(hotspot_overlay(session, reason.clone().unwrap_or_default()))
        }
        Setup::Joining { session, ssid, .. } => {
            Some(hotspot_overlay(session, format!("Joining {ssid}...")))
        }
        Setup::Joined { address, .. } => {
            let (_, secret) = remote.pairings().offer();
            Some(Overlay::Joined {
                address: address.clone(),
                payload: remote.pairing_link_at(address, &secret),
            })
        }
    }
}

/// How long a join may wait for an address before it counts as failed.
const JOIN_TIME: Duration = Duration::from_secs(30);

/// How long the phone keeps hearing from the hotspot after the TV joined a Wi-Fi, so its page can
/// say where the TV went before the hotspot drops.
const JOINED_GRACE: Duration = Duration::from_secs(5);

/// A hotspot that is up.
#[derive(Debug, Clone)]
pub(super) struct Session {
    hotspot: Hotspot,
    /// Where a phone on the hotspot opens the Remote.
    url: String,
    /// Settings asked for it, so a network does not make it go away by itself.
    on_demand: bool,
}

/// Where the Setup hotspot flow is. The hotspot comes up when there is no network (or is asked
/// for), a phone on it picks a Wi-Fi, and the TV hands over to its new address.
#[derive(Debug, Clone)]
pub(super) enum Setup {
    Off,
    Hotspot {
        session: Session,
        networks: Vec<WifiNetwork>,
        /// Why the last join failed.
        reason: Option<String>,
    },
    Joining {
        session: Session,
        ssid: String,
        waited: Duration,
    },
    /// Joined: the TV shows its new address until a phone pairs there. The hotspot stays up for
    /// `hotspot_left` more, so the phone is told.
    Joined {
        address: String,
        hotspot_left: Option<Duration>,
    },
}

impl Setup {
    /// What a phone on the hotspot is told.
    pub(super) fn view(&self) -> SetupView {
        match self {
            Setup::Off => SetupView::Off,
            Setup::Hotspot { networks, reason, .. } => SetupView::Hotspot {
                reason: reason.clone(),
                networks: networks
                    .iter()
                    .map(|n| WifiView { ssid: n.ssid.clone(), security: security_name(n.security) })
                    .collect(),
            },
            Setup::Joining { ssid, .. } => SetupView::Joining { ssid: ssid.clone() },
            Setup::Joined { address, .. } => SetupView::Joined { address: address.clone() },
        }
    }
}

impl Shell {
    /// Brings the hotspot up when there is no network, takes it down when there is one, and
    /// follows a join until it finishes or runs out of time.
    pub(super) fn drive_setup(&mut self, elapsed: Duration) {
        let network = self.os.network();
        match &mut self.setup {
            Setup::Off if network == Network::None && self.remote.is_some() => {
                self.start_hotspot(false)
            }
            Setup::Hotspot { session, .. }
                if network == Network::Ethernet
                    || (network != Network::None && !session.on_demand) =>
            {
                self.stop_hotspot()
            }
            Setup::Joining { waited, .. } => {
                *waited += elapsed;
                let timed_out = *waited >= JOIN_TIME;
                match self.os.join_status() {
                    JoinStatus::Joined { address } => {
                        self.setup = Setup::Joined { address, hotspot_left: Some(JOINED_GRACE) };
                        // The handover ends when a phone pairs at the new address, not for the
                        // one that paired on the hotspot a moment ago.
                        self.phones_seen =
                            self.remote.as_ref().map_or(0, |r| r.pairings().phones().len());
                    }
                    JoinStatus::Failed(reason) => self.hotspot_again(reason),
                    _ if timed_out => self.hotspot_again(format!(
                        "the network gave no address within {} seconds",
                        JOIN_TIME.as_secs()
                    )),
                    _ => {}
                }
            }
            // The network went away again: the hotspot is back on the next turn.
            Setup::Joined { .. } if network == Network::None => self.stop_hotspot(),
            Setup::Joined { hotspot_left: left @ Some(_), .. } => {
                let remaining = left.unwrap_or_default().saturating_sub(elapsed);
                *left = Some(remaining);
                if remaining.is_zero() {
                    *left = None;
                    self.drop_hotspot();
                }
            }
            _ => {}
        }
    }

    fn start_hotspot(&mut self, on_demand: bool) {
        let hotspot = new_hotspot();
        let address = self.os.start_hotspot(&hotspot);
        let served = self.remote.as_mut().and_then(|remote| remote.open_hotspot(address));
        // Port 80 is what a browser assumes.
        let url = served.map_or(String::new(), |served| match served.port() {
            80 => format!("http://{address}"),
            port => format!("http://{address}:{port}"),
        });
        let networks = self.os.scan_wifi();
        let session = Session { hotspot, url, on_demand };
        self.setup = Setup::Hotspot { session, networks, reason: None };
    }

    /// Stops broadcasting and serving on the hotspot.
    fn drop_hotspot(&mut self) {
        self.os.stop_hotspot();
        if let Some(remote) = &mut self.remote {
            remote.close_hotspot();
        }
    }

    fn stop_hotspot(&mut self) {
        self.drop_hotspot();
        self.setup = Setup::Off;
    }

    /// A join failed: the hotspot comes back, saying why. The phone's listener never went away.
    fn hotspot_again(&mut self, reason: String) {
        let (Setup::Joining { session, .. } | Setup::Hotspot { session, .. }) =
            std::mem::replace(&mut self.setup, Setup::Off)
        else {
            return;
        };
        // A join that was given up on must not finish later and pull the TV off the hotspot.
        self.os.cancel_join();
        self.os.start_hotspot(&session.hotspot);
        let networks = self.os.scan_wifi();
        self.setup = Setup::Hotspot { session, networks, reason: Some(reason) };
    }

    pub(super) fn join(&mut self, request: m4tv_os::JoinRequest) {
        let Setup::Hotspot { session, .. } = &self.setup else { return };
        let joining = Setup::Joining {
            session: session.clone(),
            ssid: request.ssid.clone(),
            waited: Duration::ZERO,
        };
        self.os.join_wifi(&request);
        self.setup = joining;
    }

    /// Settings starts the hotspot, and stops one it started. A TV on Ethernet has no use for
    /// one, and one that came up because there is no network goes when there is one.
    pub(super) fn ask_for_hotspot(&mut self) {
        if self.os.network() == Network::Ethernet {
            self.notice = Some("Ethernet is connected, so there is no Setup hotspot".into());
            return;
        }
        match self.setup {
            Setup::Off => self.start_hotspot(true),
            Setup::Hotspot { session: Session { on_demand: true, .. }, .. } => self.stop_hotspot(),
            _ => {}
        }
    }

    /// Ends the handover: a phone paired at the new address, or Back. The hotspot goes too, if the
    /// grace period for the phone that picked the Wi-Fi is still running.
    pub(super) fn end_handover(&mut self) {
        if matches!(self.setup, Setup::Joined { hotspot_left: Some(_), .. }) {
            self.drop_hotspot();
        }
        self.setup = Setup::Off;
        if let Some(remote) = &self.remote {
            remote.pairings().cancel();
        }
    }

    /// Back dismisses the handover. It leaves the hotspot alone: a phone may be using it.
    /// Returns whether it did anything.
    pub(super) fn put_setup_away(&mut self) -> bool {
        let handover = matches!(self.setup, Setup::Joined { .. });
        if handover {
            self.end_handover();
        }
        handover
    }
}

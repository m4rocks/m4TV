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

/// Everything the Shell needs from the device. Power calls return the message the Shell shows.
pub trait Os {
    fn settings_path(&self) -> PathBuf;
    fn version(&self) -> String;
    fn reboot(&self) -> String;
    fn power_off(&self) -> String;
}

/// The Dev host's stand-in: power actions are recorded and announced, never carried out.
/// Clones share one log, so a test can keep a handle after handing the Os to the Shell.
#[derive(Debug, Clone)]
pub struct FakeOs {
    settings_path: PathBuf,
    log: Arc<Mutex<Vec<PowerAction>>>,
}

impl FakeOs {
    pub fn new(settings_path: PathBuf) -> Self {
        Self { settings_path, log: Arc::default() }
    }

    /// Keeps the Dev host's settings in the working directory, which git ignores.
    pub fn dev_host() -> Self {
        Self::new(PathBuf::from(".m4tv-dev/settings.toml"))
    }

    pub fn actions(&self) -> Vec<PowerAction> {
        self.log.lock().unwrap().clone()
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

    #[test]
    fn the_fake_reports_its_settings_path_and_a_dev_host_version() {
        let os = FakeOs::new("dir/s.toml".into());
        assert_eq!(os.settings_path(), PathBuf::from("dir/s.toml"));
        assert!(os.version().ends_with("(Dev host)"));
    }
}

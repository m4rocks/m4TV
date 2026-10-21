//! Every setting, declared once. The Shell renders its Settings screen from `SETTINGS` and is the
//! only writer of the store; the Remote server (later) builds its screens from the same list.

use std::{collections::BTreeMap, fmt, io, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Bool(on) => f.write_str(if *on { "On" } else { "Off" }),
            Value::Int(n) => write!(f, "{n}"),
        }
    }
}

pub enum Kind {
    /// A number moved in `step`s between `min` and `max`, both included.
    Range {
        min: i64,
        max: i64,
        step: i64,
    },
    Toggle,
    /// One of `options`, held as its index; stepping past either end wraps around.
    Choice {
        options: &'static [&'static str],
    },
}

/// The group a setting belongs to on the Settings screen and in the Remote. The Network and
/// System groups hold the device actions the Shell lists itself (hotspot, power, version).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Display,
    Sound,
    Network,
    System,
}

impl Category {
    pub const ALL: [Category; 4] =
        [Category::Display, Category::Sound, Category::Network, Category::System];

    pub fn label(self) -> &'static str {
        match self {
            Category::Display => "Display",
            Category::Sound => "Sound",
            Category::Network => "Network",
            Category::System => "System",
        }
    }
}

pub struct SettingDef {
    pub key: &'static str,
    pub label: &'static str,
    pub category: Category,
    pub kind: Kind,
    pub default: Value,
}

impl SettingDef {
    /// Whether `value` is one this setting can hold: the right type, and for a range inside it
    /// and on a step.
    pub fn accepts(&self, value: Value) -> bool {
        match (&self.kind, value) {
            (Kind::Range { min, max, step }, Value::Int(n)) => {
                (*min..=*max).contains(&n) && (n - min) % step == 0
            }
            (Kind::Toggle, Value::Bool(_)) => true,
            (Kind::Choice { options }, Value::Int(n)) => (0..options.len() as i64).contains(&n),
            _ => false,
        }
    }
}

pub const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "theme",
        label: "Theme",
        category: Category::Display,
        kind: Kind::Choice { options: &["Dark", "Light"] },
        default: Value::Int(0),
    },
    SettingDef {
        key: "reduce_motion",
        label: "Reduce motion",
        category: Category::Display,
        kind: Kind::Toggle,
        default: Value::Bool(false),
    },
    SettingDef {
        key: "clock_24h",
        label: "24-hour clock",
        category: Category::Display,
        kind: Kind::Toggle,
        default: Value::Bool(true),
    },
    SettingDef {
        key: "volume",
        label: "Volume",
        category: Category::Sound,
        kind: Kind::Range { min: 0, max: 100, step: 5 },
        default: Value::Int(50),
    },
    SettingDef {
        key: "mute",
        label: "Mute",
        category: Category::Sound,
        kind: Kind::Toggle,
        default: Value::Bool(false),
    },
];

/// The current value of every declared setting.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings(BTreeMap<&'static str, Value>);

impl Settings {
    /// Reads the store. A missing or damaged file, or a value of the wrong type, gives the
    /// default; a number outside its range is clamped. Booting beats keeping a broken file.
    pub fn load(path: &Path) -> Self {
        let stored: BTreeMap<String, Value> = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        Self(
            SETTINGS
                .iter()
                .map(|def| {
                    let value = match (&def.kind, stored.get(def.key)) {
                        (Kind::Range { min, max, .. }, Some(Value::Int(n))) => {
                            Value::Int((*n).clamp(*min, *max))
                        }
                        (Kind::Toggle, Some(Value::Bool(on))) => Value::Bool(*on),
                        (Kind::Choice { options }, Some(Value::Int(n)))
                            if (0..options.len() as i64).contains(n) =>
                        {
                            Value::Int(*n)
                        }
                        _ => def.default,
                    };
                    (def.key, value)
                })
                .collect(),
        )
    }

    /// Writes the store through a temporary file, so a power cut leaves the old one intact.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(&self.0).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// The value of a declared setting.
    pub fn get(&self, key: &str) -> Value {
        self.0[key]
    }

    /// The value of a declared setting as the screens show it: a choice by its name.
    pub fn text(&self, key: &str) -> String {
        match (SETTINGS.iter().find(|def| def.key == key), self.get(key)) {
            (Some(SettingDef { kind: Kind::Choice { options }, .. }), Value::Int(n)) => {
                options[n as usize].into()
            }
            (_, value) => value.to_string(),
        }
    }

    /// Sets a declared setting to a value it accepts; anything else leaves it as it is.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Some(def) = SETTINGS.iter().find(|def| def.key == key).filter(|d| d.accepts(value)) {
            self.0.insert(def.key, value);
        }
    }

    /// Steps a setting up (`direction > 0`) or down. A toggle flips either way and a choice wraps.
    pub fn adjust(&mut self, key: &str, direction: i64) {
        let Some(def) = SETTINGS.iter().find(|def| def.key == key) else {
            return;
        };
        let value = match (&def.kind, self.get(key)) {
            (Kind::Range { min, max, step }, Value::Int(n)) => {
                Value::Int((n + direction.signum() * step).clamp(*min, *max))
            }
            (Kind::Choice { options }, Value::Int(n)) => {
                Value::Int((n + direction.signum()).rem_euclid(options.len() as i64))
            }
            (_, Value::Bool(on)) => Value::Bool(!on),
            (_, value) => value,
        };
        self.0.insert(def.key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("m4tv-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.toml")
    }

    #[test]
    fn a_missing_file_gives_the_declared_defaults() {
        let settings = Settings::load(&temp_file("missing"));
        assert_eq!(settings.get("volume"), Value::Int(50));
        assert_eq!(settings.get("mute"), Value::Bool(false));
    }

    #[test]
    fn a_saved_change_is_read_back_by_the_next_load() {
        let path = temp_file("roundtrip");
        let mut settings = Settings::load(&path);
        settings.adjust("volume", 1);
        settings.adjust("mute", 1);
        settings.save(&path).unwrap();
        let reloaded = Settings::load(&path);
        assert_eq!(reloaded.get("volume"), Value::Int(55));
        assert_eq!(reloaded.get("mute"), Value::Bool(true));
    }

    #[test]
    fn ranges_stop_at_their_ends() {
        let mut settings = Settings::load(&temp_file("clamp"));
        for _ in 0..30 {
            settings.adjust("volume", 1);
        }
        assert_eq!(settings.get("volume"), Value::Int(100));
        for _ in 0..30 {
            settings.adjust("volume", -1);
        }
        assert_eq!(settings.get("volume"), Value::Int(0));
    }

    #[test]
    fn toggles_flip_either_way() {
        let mut settings = Settings::load(&temp_file("toggle"));
        settings.adjust("mute", -1);
        assert_eq!(settings.get("mute"), Value::Bool(true));
        settings.adjust("mute", 1);
        assert_eq!(settings.get("mute"), Value::Bool(false));
    }

    #[test]
    fn a_choice_shows_its_name_wraps_at_its_ends_and_rejects_an_unknown_index() {
        let path = temp_file("choice");
        let mut settings = Settings::load(&path);
        assert_eq!(settings.text("theme"), "Dark");
        settings.adjust("theme", 1);
        assert_eq!(settings.text("theme"), "Light");
        settings.adjust("theme", 1);
        assert_eq!(settings.text("theme"), "Dark");
        settings.adjust("theme", -1);
        assert_eq!(settings.text("theme"), "Light");
        settings.set("theme", Value::Int(2));
        assert_eq!(settings.text("theme"), "Light", "an index with no option is refused");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "theme = 9").unwrap();
        assert_eq!(Settings::load(&path).text("theme"), "Dark");
    }

    #[test]
    fn the_display_settings_default_to_dark_motion_on_and_a_24_hour_clock() {
        let settings = Settings::load(&temp_file("display"));
        assert_eq!(settings.get("reduce_motion"), Value::Bool(false));
        assert_eq!(settings.get("clock_24h"), Value::Bool(true));
        let display: Vec<_> =
            SETTINGS.iter().filter(|d| d.category == Category::Display).map(|d| d.key).collect();
        assert_eq!(display, ["theme", "reduce_motion", "clock_24h"]);
    }

    #[test]
    fn a_damaged_file_falls_back_to_defaults_instead_of_stopping_boot() {
        let path = temp_file("damaged");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for text in ["not toml {{", "volume = \"loud\"\nmute = 3", "volume = 900\nextra = 1"] {
            std::fs::write(&path, text).unwrap();
            let settings = Settings::load(&path);
            assert_eq!(settings.get("mute"), Value::Bool(false), "{text:?}");
            assert!(matches!(settings.get("volume"), Value::Int(0..=100)), "{text:?}");
        }
    }
}

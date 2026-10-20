//! App definitions: declarative data shipped with the Shell, never code.

use serde::Deserialize;

/// What the Shell needs to present a web service as an App.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppDefinition {
    /// Stable key for the App: a lowercase slug, unique in the catalogue.
    pub id: String,
    pub name: String,
    pub icon: String,
    pub start_url: String,
    pub user_agent: Option<String>,
    pub css: Option<String>,
    pub js: Option<String>,
    #[serde(default)]
    pub tv: TvTuning,
}

/// TV tuning flags. Spatial navigation is on unless an App turns it off (#13).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TvTuning {
    pub spatial_nav: bool,
}

impl Default for TvTuning {
    fn default() -> Self {
        Self { spatial_nav: true }
    }
}

#[derive(Deserialize)]
struct Catalog {
    app: Vec<AppDefinition>,
}

/// Parses a catalogue, rejecting any `id` that is not a lowercase slug or is repeated.
pub fn parse_catalog(text: &str) -> Result<Vec<AppDefinition>, String> {
    let apps = toml::from_str::<Catalog>(text).map_err(|e| e.to_string())?.app;
    for (i, app) in apps.iter().enumerate() {
        if !is_slug(&app.id) {
            return Err(format!("App id {:?} is not a lowercase slug", app.id));
        }
        if apps[..i].iter().any(|other| other.id == app.id) {
            return Err(format!("App id {:?} is repeated", app.id));
        }
    }
    Ok(apps)
}

/// Lowercase letters and digits, in groups separated by single hyphens.
fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// The three Apps v1 ships. The data is embedded, so a parse error is a build-time mistake.
pub fn built_in_apps() -> Vec<AppDefinition> {
    parse_catalog(include_str!("../data/apps.toml")).expect("data/apps.toml is valid")
}

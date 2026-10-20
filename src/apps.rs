//! App definitions: declarative data shipped with the Shell, never code.

use serde::Deserialize;

/// What the Shell needs to present a web service as an App.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppDefinition {
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
#[serde(deny_unknown_fields)]
pub struct TvTuning {
    #[serde(default = "yes")]
    pub spatial_nav: bool,
}

impl Default for TvTuning {
    fn default() -> Self {
        Self { spatial_nav: true }
    }
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct Catalog {
    app: Vec<AppDefinition>,
}

/// The three Apps v1 ships. The data is embedded, so a parse error is a build-time mistake.
pub fn built_in_apps() -> Vec<AppDefinition> {
    toml::from_str::<Catalog>(include_str!("../data/apps.toml"))
        .expect("data/apps.toml is valid")
        .app
}

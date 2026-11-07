use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct ThemeTokens {
    pub light: HashMap<String, String>,
    pub dark: HashMap<String, String>,
    pub radius: String,
    pub font_sans: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_mono: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeResponse {
    pub name: String,
    pub label: String,
    pub color_mode: String,
    pub source: String,
    pub tokens: ThemeTokens,
    /// Which layout component renders the app (`default`, `desk`, `custom`, …).
    pub layout: String,
    /// Which set of error pages (404, 403, error cards, …) the app shows.
    pub error_pages: String,
    /// Navigation for the layout's navbar; `None` uses the layout's built-in one.
    pub nav: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeListItem {
    pub name: String,
    pub label: String,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeListResponse {
    pub themes: Vec<ThemeListItem>,
    pub active: String,
    pub source: String,
}

/// The built-in default theme's manifest, shared with the web app (`lib/theme/default.ts`)
/// so the fallback the server sends and the theme the app bakes in cannot drift apart.
pub const DEFAULT_THEME_JSON: &str = include_str!("../../../../web/src/lib/theme/default-theme.json");

#[derive(Debug, Deserialize)]
struct DefaultManifest {
    name: String,
    label: String,
    color_mode: String,
    radius: String,
    font_sans: String,
    font_mono: Option<String>,
    density: Option<String>,
    layout: String,
    light: HashMap<String, String>,
    dark: HashMap<String, String>,
}

/// The built-in default theme, served when an organization has no theme of its own.
pub fn default_theme(source: &str) -> ThemeResponse {
    let manifest: DefaultManifest = serde_json::from_str(DEFAULT_THEME_JSON)
        .unwrap_or_else(|error| panic!("default-theme.json is not a theme manifest: {error}"));
    ThemeResponse {
        name: manifest.name,
        label: manifest.label,
        color_mode: manifest.color_mode,
        source: source.into(),
        tokens: ThemeTokens {
            light: manifest.light,
            dark: manifest.dark,
            radius: manifest.radius,
            font_sans: manifest.font_sans,
            font_mono: manifest.font_mono,
            density: manifest.density,
        },
        layout: manifest.layout,
        error_pages: "default".into(),
        nav: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_reads_the_shared_manifest() {
        let theme = default_theme("fallback");
        assert_eq!(theme.name, "default");
        assert_eq!(theme.source, "fallback");
        assert!(theme.tokens.light.contains_key("primary"));
        assert_eq!(
            theme.tokens.light.keys().len(),
            theme.tokens.dark.keys().len()
        );
    }
}

use axum::{Json, Router, routing::get};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct ThemeTokens {
    pub light: HashMap<String, String>,
    pub dark: HashMap<String, String>,
    pub radius: String,
    pub font_sans: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeResponse {
    pub name: String,
    pub label: String,
    pub color_mode: String,
    pub source: String,
    pub tokens: ThemeTokens,
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

pub fn router() -> Router {
    Router::new()
        .route("/theme", get(get_theme))
        .route("/themes", get(list_themes))
}

async fn get_theme() -> Json<ThemeResponse> {
    Json(enterprise_theme("fallback"))
}

async fn list_themes() -> Json<ThemeListResponse> {
    Json(ThemeListResponse {
        themes: vec![ThemeListItem {
            name: "enterprise".into(),
            label: "Enterprise".into(),
            is_system: true,
        }],
        active: "enterprise".into(),
        source: "fallback".into(),
    })
}

pub fn enterprise_theme(source: &str) -> ThemeResponse {
    ThemeResponse {
        name: "enterprise".into(),
        label: "Enterprise".into(),
        color_mode: "system".into(),
        source: source.into(),
        tokens: ThemeTokens {
            light: enterprise_light(),
            dark: enterprise_dark(),
            radius: "0.375rem".into(),
            font_sans: "\"Noto Sans Variable\", \"Segoe UI\", system-ui, sans-serif".into(),
        },
    }
}

fn enterprise_light() -> HashMap<String, String> {
    HashMap::from([
        ("background".into(), "oklch(0.985 0.002 247)".into()),
        ("foreground".into(), "oklch(0.22 0.02 255)".into()),
        ("card".into(), "oklch(1 0 0)".into()),
        ("card-foreground".into(), "oklch(0.22 0.02 255)".into()),
        ("popover".into(), "oklch(1 0 0)".into()),
        ("popover-foreground".into(), "oklch(0.22 0.02 255)".into()),
        ("primary".into(), "oklch(0.45 0.14 255)".into()),
        ("primary-foreground".into(), "oklch(0.99 0.01 255)".into()),
        ("secondary".into(), "oklch(0.94 0.01 255)".into()),
        ("secondary-foreground".into(), "oklch(0.28 0.03 255)".into()),
        ("muted".into(), "oklch(0.95 0.008 250)".into()),
        ("muted-foreground".into(), "oklch(0.48 0.02 255)".into()),
        ("accent".into(), "oklch(0.93 0.02 250)".into()),
        ("accent-foreground".into(), "oklch(0.28 0.03 255)".into()),
        ("destructive".into(), "oklch(0.55 0.2 25)".into()),
        ("border".into(), "oklch(0.88 0.01 255)".into()),
        ("input".into(), "oklch(0.88 0.01 255)".into()),
        ("ring".into(), "oklch(0.55 0.12 255)".into()),
        ("sidebar".into(), "oklch(0.24 0.03 255)".into()),
        ("sidebar-foreground".into(), "oklch(0.93 0.01 255)".into()),
        ("sidebar-primary".into(), "oklch(0.62 0.14 255)".into()),
        ("sidebar-primary-foreground".into(), "oklch(0.99 0.01 255)".into()),
        ("sidebar-accent".into(), "oklch(0.32 0.03 255)".into()),
        ("sidebar-accent-foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("sidebar-border".into(), "oklch(0.34 0.02 255)".into()),
        ("sidebar-ring".into(), "oklch(0.62 0.14 255)".into()),
    ])
}

fn enterprise_dark() -> HashMap<String, String> {
    HashMap::from([
        ("background".into(), "oklch(0.18 0.02 255)".into()),
        ("foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("card".into(), "oklch(0.22 0.02 255)".into()),
        ("card-foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("popover".into(), "oklch(0.22 0.02 255)".into()),
        ("popover-foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("primary".into(), "oklch(0.68 0.13 255)".into()),
        ("primary-foreground".into(), "oklch(0.18 0.02 255)".into()),
        ("secondary".into(), "oklch(0.28 0.02 255)".into()),
        ("secondary-foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("muted".into(), "oklch(0.28 0.02 255)".into()),
        ("muted-foreground".into(), "oklch(0.7 0.02 255)".into()),
        ("accent".into(), "oklch(0.3 0.03 255)".into()),
        ("accent-foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("destructive".into(), "oklch(0.65 0.18 25)".into()),
        ("border".into(), "oklch(1 0 0 / 12%)".into()),
        ("input".into(), "oklch(1 0 0 / 14%)".into()),
        ("ring".into(), "oklch(0.58 0.12 255)".into()),
        ("sidebar".into(), "oklch(0.16 0.025 255)".into()),
        ("sidebar-foreground".into(), "oklch(0.93 0.01 255)".into()),
        ("sidebar-primary".into(), "oklch(0.68 0.13 255)".into()),
        ("sidebar-primary-foreground".into(), "oklch(0.16 0.025 255)".into()),
        ("sidebar-accent".into(), "oklch(0.24 0.03 255)".into()),
        ("sidebar-accent-foreground".into(), "oklch(0.95 0.01 255)".into()),
        ("sidebar-border".into(), "oklch(1 0 0 / 10%)".into()),
        ("sidebar-ring".into(), "oklch(0.68 0.13 255)".into()),
    ])
}

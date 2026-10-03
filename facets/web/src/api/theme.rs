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

pub fn enterprise_theme(source: &str) -> ThemeResponse {
    ThemeResponse {
        name: "enterprise".into(),
        label: "Enterprise".into(),
        color_mode: "system".into(),
        source: source.into(),
        tokens: ThemeTokens {
            light: enterprise_light(),
            dark: enterprise_dark(),
            radius: "0.25rem".into(),
            font_sans: "\"Noto Sans Variable\", \"Segoe UI\", system-ui, sans-serif".into(),
        },
        layout: "default".into(),
        error_pages: "default".into(),
        nav: None,
    }
}

fn enterprise_light() -> HashMap<String, String> {
    HashMap::from([
        ("background".into(), "oklch(0.971 0.006 185)".into()),
        ("foreground".into(), "oklch(0.229 0.022 191)".into()),
        ("card".into(), "oklch(1.000 0.000 90)".into()),
        ("card-foreground".into(), "oklch(0.229 0.022 191)".into()),
        ("popover".into(), "oklch(1.000 0.000 90)".into()),
        ("popover-foreground".into(), "oklch(0.229 0.022 191)".into()),
        ("primary".into(), "oklch(0.511 0.086 186)".into()),
        ("primary-foreground".into(), "oklch(0.981 0.010 189)".into()),
        ("secondary".into(), "oklch(0.947 0.011 183)".into()),
        ("secondary-foreground".into(), "oklch(0.345 0.047 189)".into()),
        ("muted".into(), "oklch(0.955 0.009 180)".into()),
        ("muted-foreground".into(), "oklch(0.510 0.025 189)".into()),
        ("accent".into(), "oklch(0.940 0.021 182)".into()),
        ("accent-foreground".into(), "oklch(0.387 0.063 187)".into()),
        ("destructive".into(), "oklch(0.55 0.21 25)".into()),
        ("border".into(), "oklch(0.917 0.014 181)".into()),
        ("input".into(), "oklch(0.890 0.018 179)".into()),
        ("ring".into(), "oklch(0.604 0.103 185)".into()),
        ("highlight".into(), "oklch(0.769 0.165 70)".into()),
        ("highlight-foreground".into(), "oklch(0.234 0.049 76)".into()),
        ("sidebar".into(), "oklch(0.330 0.048 197)".into()),
        ("sidebar-foreground".into(), "oklch(0.912 0.023 186)".into()),
        ("sidebar-primary".into(), "oklch(0.769 0.165 70)".into()),
        ("sidebar-primary-foreground".into(), "oklch(0.234 0.049 76)".into()),
        ("sidebar-accent".into(), "oklch(0.411 0.060 205)".into()),
        ("sidebar-accent-foreground".into(), "oklch(0.981 0.010 189)".into()),
        ("sidebar-border".into(), "oklch(0.273 0.039 198)".into()),
        ("sidebar-ring".into(), "oklch(0.769 0.165 70)".into()),
    ])
}

fn enterprise_dark() -> HashMap<String, String> {
    HashMap::from([
        ("background".into(), "oklch(0.194 0.016 196)".into()),
        ("foreground".into(), "oklch(0.949 0.012 184)".into()),
        ("card".into(), "oklch(0.234 0.021 191)".into()),
        ("card-foreground".into(), "oklch(0.949 0.012 184)".into()),
        ("popover".into(), "oklch(0.264 0.025 191)".into()),
        ("popover-foreground".into(), "oklch(0.949 0.012 184)".into()),
        ("primary".into(), "oklch(0.785 0.133 182)".into()),
        ("primary-foreground".into(), "oklch(0.280 0.046 186)".into()),
        ("secondary".into(), "oklch(0.285 0.025 192)".into()),
        ("secondary-foreground".into(), "oklch(0.949 0.012 184)".into()),
        ("muted".into(), "oklch(0.285 0.025 192)".into()),
        ("muted-foreground".into(), "oklch(0.714 0.029 188)".into()),
        ("accent".into(), "oklch(0.338 0.039 188)".into()),
        ("accent-foreground".into(), "oklch(0.949 0.012 184)".into()),
        ("destructive".into(), "oklch(0.65 0.19 25)".into()),
        ("border".into(), "oklch(1 0 0 / 9%)".into()),
        ("input".into(), "oklch(1 0 0 / 12%)".into()),
        ("ring".into(), "oklch(0.785 0.133 182)".into()),
        ("highlight".into(), "oklch(0.837 0.164 84)".into()),
        ("highlight-foreground".into(), "oklch(0.234 0.049 76)".into()),
        ("sidebar".into(), "oklch(0.221 0.030 195)".into()),
        ("sidebar-foreground".into(), "oklch(0.905 0.024 187)".into()),
        ("sidebar-primary".into(), "oklch(0.837 0.164 84)".into()),
        ("sidebar-primary-foreground".into(), "oklch(0.234 0.049 76)".into()),
        ("sidebar-accent".into(), "oklch(0.321 0.044 197)".into()),
        ("sidebar-accent-foreground".into(), "oklch(0.981 0.010 189)".into()),
        ("sidebar-border".into(), "oklch(1 0 0 / 8%)".into()),
        ("sidebar-ring".into(), "oklch(0.837 0.164 84)".into()),
    ])
}

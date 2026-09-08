-- ================================================
-- UI theme config
-- Which theme is active for this org (and optional
-- per-user overrides later).
-- ================================================

DEFINE TABLE IF NOT EXISTS ui_theme_config;

DEFINE FIELD IF NOT EXISTS active_theme ON TABLE ui_theme_config TYPE record<ui_themes>;
-- Optional token overrides layered on top of the theme
DEFINE FIELD IF NOT EXISTS overrides ON TABLE ui_theme_config TYPE option<object | NONE>;
-- "light" | "dark" | "system"
DEFINE FIELD IF NOT EXISTS color_mode ON TABLE ui_theme_config TYPE string DEFAULT "system";
DEFINE FIELD IF NOT EXISTS date_created ON TABLE ui_theme_config TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE ui_theme_config TYPE datetime VALUE time::now();

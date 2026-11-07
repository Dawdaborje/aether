-- ================================================
-- Theme layout and navigation (per organization)
--
-- A theme installed from a theme plugin brings more than colours: the layout
-- component that renders the app and the navigation it shows. The active theme
-- is `ui_theme_config:org`.
-- ================================================

DEFINE FIELD IF NOT EXISTS color_mode ON TABLE ui_themes TYPE string DEFAULT "system";
DEFINE FIELD IF NOT EXISTS layout ON TABLE ui_themes TYPE string DEFAULT "default";
DEFINE FIELD IF NOT EXISTS nav ON TABLE ui_themes TYPE option<object | NONE>;
-- Which plugin (and version) the theme came from.
DEFINE FIELD IF NOT EXISTS plugin_name ON TABLE ui_themes TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin_version ON TABLE ui_themes TYPE option<string | NONE>;

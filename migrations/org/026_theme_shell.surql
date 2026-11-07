-- Theme manifest v2: shell settings beyond colours (home page, branding),
-- copied from plugin_themes when a theme plugin is installed.

DEFINE FIELD IF NOT EXISTS shell ON TABLE ui_themes TYPE option<object | NONE>;

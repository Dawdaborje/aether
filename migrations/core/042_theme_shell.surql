-- Theme manifest v2: shell settings beyond colours (home page, branding).
-- See facets/core/src/plugin_manager/themes.rs.

DEFINE FIELD IF NOT EXISTS shell ON TABLE plugin_themes TYPE option<object | NONE>;

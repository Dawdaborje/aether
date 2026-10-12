-- Layout and error-page definitions a plugin ships (JSON under layouts/ and errors/).
-- Themes may name them; the resolved definition is stored on the theme row too.

DEFINE TABLE IF NOT EXISTS plugin_layouts;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_layouts TYPE record<plugins>;
DEFINE FIELD IF NOT EXISTS name ON TABLE plugin_layouts TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugin_layouts TYPE string;
DEFINE FIELD IF NOT EXISTS chrome ON TABLE plugin_layouts TYPE string;
DEFINE FIELD IF NOT EXISTS definition ON TABLE plugin_layouts TYPE object;
DEFINE FIELD IF NOT EXISTS source_path ON TABLE plugin_layouts TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_layouts TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS plugin_layouts_plugin_name ON TABLE plugin_layouts COLUMNS plugin, name UNIQUE;

DEFINE TABLE IF NOT EXISTS plugin_error_sets;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_error_sets TYPE record<plugins>;
DEFINE FIELD IF NOT EXISTS name ON TABLE plugin_error_sets TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugin_error_sets TYPE string;
DEFINE FIELD IF NOT EXISTS variant ON TABLE plugin_error_sets TYPE string;
DEFINE FIELD IF NOT EXISTS definition ON TABLE plugin_error_sets TYPE object;
DEFINE FIELD IF NOT EXISTS source_path ON TABLE plugin_error_sets TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_error_sets TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS plugin_error_sets_plugin_name ON TABLE plugin_error_sets COLUMNS plugin, name UNIQUE;

DEFINE FIELD IF NOT EXISTS layout_def ON TABLE plugin_themes TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS error_pages_def ON TABLE plugin_themes TYPE option<object | NONE>;

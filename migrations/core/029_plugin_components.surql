-- ================================================
-- Plugin UI components
-- Reusable XML fragments under components/,
-- registered in the platform catalog per plugin version.
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_components;

DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_components TYPE record<plugins>;
-- Local short name, e.g. "note_card"
DEFINE FIELD IF NOT EXISTS name ON TABLE plugin_components TYPE string;
-- Qualified name: <plugin>.<name>, e.g. "notes.note_card"
DEFINE FIELD IF NOT EXISTS full_name ON TABLE plugin_components TYPE string;
-- Compiled component tree JSON from the XML definition
DEFINE FIELD IF NOT EXISTS component_tree ON TABLE plugin_components TYPE option<object | NONE>;
-- Source path of the .xml file inside the package
DEFINE FIELD IF NOT EXISTS source_path ON TABLE plugin_components TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_components TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugin_components TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugin_components_plugin_name
    ON TABLE plugin_components COLUMNS plugin, name UNIQUE;
DEFINE INDEX IF NOT EXISTS plugin_components_full_name
    ON TABLE plugin_components COLUMNS full_name;

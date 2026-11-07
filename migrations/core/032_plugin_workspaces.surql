-- ================================================
-- Plugin workspaces (launcher groups)
-- Metadata from workspace.toml: group_label / group_icon
-- for the Apps launcher folders.
-- ================================================

DEFINE TABLE IF NOT EXISTS plugin_workspaces;

DEFINE FIELD IF NOT EXISTS name ON TABLE plugin_workspaces TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugin_workspaces TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS group_label ON TABLE plugin_workspaces TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS group_icon ON TABLE plugin_workspaces TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS description ON TABLE plugin_workspaces TYPE option<string | NONE>;
-- Relative path under app_dir/workspaces/<name>/ when a logo file was copied.
DEFINE FIELD IF NOT EXISTS logo_file ON TABLE plugin_workspaces TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_workspaces TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugin_workspaces TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugin_workspaces_name ON TABLE plugin_workspaces COLUMNS name UNIQUE;

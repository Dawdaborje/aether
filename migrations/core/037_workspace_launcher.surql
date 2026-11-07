-- ================================================
-- Workspace launcher mode: folder (default) or flat.
-- Flat workspaces (e.g. Base) list each app on the home grid.
-- ================================================

DEFINE FIELD IF NOT EXISTS launcher ON TABLE plugin_workspaces TYPE option<string | NONE>;

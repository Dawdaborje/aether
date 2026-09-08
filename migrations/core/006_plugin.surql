-- ================================================
-- Plugins (platform catalog)
-- Registered plugin packages available to the kernel.
-- Per-org installs live in org_migrations
-- (installed_plugins).
-- ================================================

DEFINE TABLE IF NOT EXISTS plugins;

DEFINE FIELD IF NOT EXISTS name ON TABLE plugins TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE plugins TYPE string;
DEFINE FIELD IF NOT EXISTS version ON TABLE plugins TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE plugins TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS long_description ON TABLE plugins TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS icon_path ON TABLE plugins TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS website ON TABLE plugins TYPE option<string | NONE>;
-- Authors as [{ name, email, github?, website? }, …]
DEFINE FIELD IF NOT EXISTS authors ON TABLE plugins TYPE option<array<object> | NONE>;
-- Categories as [{ name, label }, …]
DEFINE FIELD IF NOT EXISTS categories ON TABLE plugins TYPE option<array<object> | NONE>;
-- Other plugin names this package depends on (denormalized
-- cache of the plugin_depends_on graph for quick reads).
-- Prefer graph queries for install order and reverse lookup;
-- see migrations/012_plugin_dependency_graph.surql.
DEFINE FIELD IF NOT EXISTS dependencies ON TABLE plugins TYPE option<array<string> | NONE>;
-- Workspace this addon belongs to (e.g. "base", "erp")
DEFINE FIELD IF NOT EXISTS workspace ON TABLE plugins TYPE option<string | NONE>;
-- "addon" | "theme" | "bridge_pack" | …
DEFINE FIELD IF NOT EXISTS kind ON TABLE plugins TYPE option<string | NONE> DEFAULT "addon";
-- Path or content hash of the WASM / archive artifact
DEFINE FIELD IF NOT EXISTS artifact_path ON TABLE plugins TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS artifact_hash ON TABLE plugins TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_builtin ON TABLE plugins TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS is_active ON TABLE plugins TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugins TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE plugins TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS plugins_name ON TABLE plugins COLUMNS name UNIQUE;
DEFINE INDEX IF NOT EXISTS plugins_workspace ON TABLE plugins COLUMNS workspace;
DEFINE INDEX IF NOT EXISTS plugins_kind ON TABLE plugins COLUMNS kind;

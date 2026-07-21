-- ================================================
-- Bridge configs
-- Per-org credentials and enable flags for platform
-- bridges. Catalog rows live in core `bridges`.
-- ================================================

DEFINE TABLE IF NOT EXISTS bridge_configs;

-- Matches core bridges.name / feature_key
DEFINE FIELD IF NOT EXISTS bridge_name ON TABLE bridge_configs TYPE string;
DEFINE FIELD IF NOT EXISTS is_enabled ON TABLE bridge_configs TYPE bool DEFAULT false;
-- Encrypted credentials and bridge-specific settings
DEFINE FIELD IF NOT EXISTS credentials_encrypted ON TABLE bridge_configs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS config ON TABLE bridge_configs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE bridge_configs TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE bridge_configs TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS bridge_configs_name
    ON TABLE bridge_configs COLUMNS bridge_name UNIQUE;

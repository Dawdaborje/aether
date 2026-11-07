-- A plugin name may have multiple immutable catalog versions.
-- Organizations pin a version in their installed_plugins record.
REMOVE INDEX plugins_name ON TABLE plugins;

DEFINE INDEX IF NOT EXISTS plugins_name_version_unique
    ON TABLE plugins COLUMNS name, version UNIQUE;
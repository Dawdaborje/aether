-- ================================================
-- Schema migrations (org)
-- Tracks which org migration files have been applied
-- to this tenant database.
-- ================================================

DEFINE TABLE IF NOT EXISTS schema_migrations;

DEFINE FIELD IF NOT EXISTS version ON TABLE schema_migrations TYPE string;
DEFINE FIELD IF NOT EXISTS filename ON TABLE schema_migrations TYPE string;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE schema_migrations TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS checksum ON TABLE schema_migrations TYPE option<string | NONE>;

DEFINE INDEX IF NOT EXISTS schema_migrations_version ON TABLE schema_migrations COLUMNS version UNIQUE;
DEFINE INDEX IF NOT EXISTS schema_migrations_filename ON TABLE schema_migrations COLUMNS filename UNIQUE;

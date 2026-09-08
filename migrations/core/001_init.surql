-- ==========================================
-- ORGANIZATIONS DATABASE
-- The database that keeps track of all organization databases for multi-tenancy.
-- ==========================================

DEFINE TABLE IF NOT EXISTS org_databases;

DEFINE FIELD IF NOT EXISTS db_name ON TABLE org_databases TYPE string
  VALUE string::replace($value, " ", "_");
DEFINE FIELD IF NOT EXISTS date_created ON TABLE org_databases TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE org_databases TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS org_database_db_name ON TABLE org_databases COLUMNS db_name UNIQUE;

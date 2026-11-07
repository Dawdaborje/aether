-- Core organization catalog and cross-database membership links.
-- Companies are not a kernel concept: they belong to the `company` plugin.

DEFINE TABLE IF NOT EXISTS organizations;
DEFINE FIELD IF NOT EXISTS name ON TABLE organizations TYPE string;
DEFINE FIELD IF NOT EXISTS db_name ON TABLE organizations TYPE string;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE organizations TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE organizations TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS organizations_db_name ON TABLE organizations COLUMNS db_name UNIQUE;

DEFINE TABLE IF NOT EXISTS organization_users;
DEFINE FIELD IF NOT EXISTS organization_id ON TABLE organization_users TYPE string;
DEFINE FIELD IF NOT EXISTS user_id ON TABLE organization_users TYPE string;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE organization_users TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS organization_users_unique ON TABLE organization_users COLUMNS organization_id, user_id UNIQUE;

-- Clean up databases created while companies were still part of the kernel
-- (no-ops on a fresh database).
REMOVE FIELD IF EXISTS company_id ON TABLE organizations;
REMOVE TABLE IF EXISTS company_users;
REMOVE TABLE IF EXISTS companies;

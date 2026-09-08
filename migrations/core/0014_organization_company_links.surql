-- Core organization catalog and cross-database membership links.

DEFINE TABLE IF NOT EXISTS organizations;
DEFINE FIELD IF NOT EXISTS name ON TABLE organizations TYPE string;
DEFINE FIELD IF NOT EXISTS db_name ON TABLE organizations TYPE string;
DEFINE FIELD IF NOT EXISTS company_id ON TABLE organizations TYPE string;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE organizations TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE organizations TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS organizations_db_name ON TABLE organizations COLUMNS db_name UNIQUE;

DEFINE TABLE IF NOT EXISTS company_users;
DEFINE FIELD IF NOT EXISTS company_id ON TABLE company_users TYPE string;
DEFINE FIELD IF NOT EXISTS user_id ON TABLE company_users TYPE string;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE company_users TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS company_users_unique ON TABLE company_users COLUMNS company_id, user_id UNIQUE;

DEFINE TABLE IF NOT EXISTS organization_users;
DEFINE FIELD IF NOT EXISTS organization_id ON TABLE organization_users TYPE string;
DEFINE FIELD IF NOT EXISTS user_id ON TABLE organization_users TYPE string;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE organization_users TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS organization_users_unique ON TABLE organization_users COLUMNS organization_id, user_id UNIQUE;
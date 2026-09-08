-- ================================================
-- Audit logs (platform)
-- Kernel / facet actions across the platform.
-- Org-scoped business audit can live in the tenant DB
-- later if needed; this is the platform trail.
-- ================================================

DEFINE TABLE IF NOT EXISTS audit_logs;

DEFINE FIELD IF NOT EXISTS actor_user_id ON TABLE audit_logs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS actor_type ON TABLE audit_logs TYPE string DEFAULT "user";
-- e.g. "user.login", "org.create", "plugin.install"
DEFINE FIELD IF NOT EXISTS action ON TABLE audit_logs TYPE string;
DEFINE FIELD IF NOT EXISTS resource_type ON TABLE audit_logs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS resource_id ON TABLE audit_logs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS org_database_id ON TABLE audit_logs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS ip_address ON TABLE audit_logs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS user_agent ON TABLE audit_logs TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS metadata ON TABLE audit_logs TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE audit_logs TYPE datetime DEFAULT time::now();

DEFINE INDEX IF NOT EXISTS audit_logs_action ON TABLE audit_logs COLUMNS action;
DEFINE INDEX IF NOT EXISTS audit_logs_actor ON TABLE audit_logs COLUMNS actor_user_id;
DEFINE INDEX IF NOT EXISTS audit_logs_org ON TABLE audit_logs COLUMNS org_database_id;

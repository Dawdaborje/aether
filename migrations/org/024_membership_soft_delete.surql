-- Soft-delete for org membership and role grants (kick / revoke leave rows).

DEFINE FIELD IF NOT EXISTS deleted_at ON TABLE org_users TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS deleted_by ON TABLE org_users TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS org_users_deleted ON TABLE org_users COLUMNS deleted_at;

DEFINE FIELD IF NOT EXISTS deleted_at ON TABLE org_user_roles TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS deleted_by ON TABLE org_user_roles TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS org_user_roles_deleted ON TABLE org_user_roles COLUMNS deleted_at;

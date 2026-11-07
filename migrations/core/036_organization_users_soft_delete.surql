-- Soft-delete for membership links (kick leaves the row).

DEFINE FIELD IF NOT EXISTS deleted_at ON TABLE organization_users TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS deleted_by ON TABLE organization_users TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS organization_users_deleted ON TABLE organization_users COLUMNS deleted_at;

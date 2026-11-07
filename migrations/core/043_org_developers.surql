-- Org-scoped developers: a member marked `is_developer` gets developer tools (schedules) in that
-- organization only. Unrestricted developers stay `users.is_super_user`. Only an unrestricted
-- developer sets this flag. See facets/core/src/org_admin.rs (`is_org_developer`).

DEFINE FIELD IF NOT EXISTS is_developer ON TABLE organization_users TYPE option<bool | NONE>;

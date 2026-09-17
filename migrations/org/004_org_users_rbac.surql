-- ================================================
-- Org users & RBAC
-- Per-tenant mirror of core users plus roles, groups,
-- and permissions scoped to this organization.
--
-- Core users (migrations/005_users.surql) hold identity.
-- This file holds "what can this user do in THIS org".
-- Platform superusers bypass all of these checks.
-- ================================================

DEFINE TABLE IF NOT EXISTS org_users;

-- Record id string of the core `users` row (e.g. "users:xxx")
DEFINE FIELD IF NOT EXISTS core_user_id ON TABLE org_users TYPE string;
DEFINE FIELD IF NOT EXISTS display_name ON TABLE org_users TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_active ON TABLE org_users TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE org_users TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE org_users TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS org_users_core_user ON TABLE org_users COLUMNS core_user_id UNIQUE;


-- ================================================
-- Permissions (org catalog)
-- ================================================

DEFINE TABLE IF NOT EXISTS permissions;

DEFINE FIELD IF NOT EXISTS s_key ON TABLE permissions TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE permissions TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE permissions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE permissions TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE permissions TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS permissions_key ON TABLE permissions COLUMNS s_key UNIQUE;


-- ================================================
-- Roles
-- ================================================

DEFINE TABLE IF NOT EXISTS roles;

DEFINE FIELD IF NOT EXISTS name ON TABLE roles TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE roles TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE roles TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_system ON TABLE roles TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE roles TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE roles TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS roles_name ON TABLE roles COLUMNS name UNIQUE;


DEFINE TABLE IF NOT EXISTS role_permissions;

DEFINE FIELD IF NOT EXISTS role ON TABLE role_permissions TYPE record<roles>;
DEFINE FIELD IF NOT EXISTS permission ON TABLE role_permissions TYPE record<permissions>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE role_permissions TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE role_permissions TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS role_permissions_unique
    ON TABLE role_permissions COLUMNS role, permission UNIQUE;


-- ================================================
-- Groups
-- ================================================

DEFINE TABLE IF NOT EXISTS groups;

DEFINE FIELD IF NOT EXISTS name ON TABLE groups TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE groups TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE groups TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_system ON TABLE groups TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE groups TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE groups TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS groups_name ON TABLE groups COLUMNS name UNIQUE;


DEFINE TABLE IF NOT EXISTS group_roles;

DEFINE FIELD IF NOT EXISTS group ON TABLE group_roles TYPE record<groups>;
DEFINE FIELD IF NOT EXISTS role ON TABLE group_roles TYPE record<roles>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE group_roles TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE group_roles TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS group_roles_unique
    ON TABLE group_roles COLUMNS group, role UNIQUE;


DEFINE TABLE IF NOT EXISTS org_user_groups;

DEFINE FIELD IF NOT EXISTS org_user ON TABLE org_user_groups TYPE record<org_users>;
DEFINE FIELD IF NOT EXISTS group ON TABLE org_user_groups TYPE record<groups>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE org_user_groups TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE org_user_groups TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS org_user_groups_unique
    ON TABLE org_user_groups COLUMNS org_user, group UNIQUE;


DEFINE TABLE IF NOT EXISTS org_user_roles;

DEFINE FIELD IF NOT EXISTS org_user ON TABLE org_user_roles TYPE record<org_users>;
DEFINE FIELD IF NOT EXISTS role ON TABLE org_user_roles TYPE record<roles>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE org_user_roles TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE org_user_roles TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS org_user_roles_unique
    ON TABLE org_user_roles COLUMNS org_user, role UNIQUE;

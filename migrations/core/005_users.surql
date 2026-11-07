-- ================================================
-- Users
-- Core identity lives here. Org-scoped roles/groups
-- live in org_migrations (per-tenant DB).
--
-- Superuser rule: when is_super_user = true the kernel
-- MUST bypass every RBAC / capability check. A superuser
-- can do anything across all orgs.
-- ================================================

DEFINE TABLE IF NOT EXISTS users;

DEFINE FIELD IF NOT EXISTS username ON TABLE users TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS email ON TABLE users TYPE option<string | NONE>
    ASSERT $value = NONE OR string::is_email($value);
DEFINE FIELD IF NOT EXISTS display_name ON TABLE users TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS avatar_url ON TABLE users TYPE option<string | NONE>;

-- Local password auth. Optional so social / IdP-only
-- accounts (Keycloak, Google, etc.) can exist without one.
DEFINE FIELD IF NOT EXISTS hashed_password ON TABLE users TYPE option<string | NONE>;

DEFINE FIELD IF NOT EXISTS is_super_user ON TABLE users TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS is_active ON TABLE users TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS is_email_verified ON TABLE users TYPE bool DEFAULT false;

DEFINE FIELD IF NOT EXISTS last_login_at ON TABLE users TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE users TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE users TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS users_email ON TABLE users COLUMNS email UNIQUE;
DEFINE INDEX IF NOT EXISTS users_username ON TABLE users COLUMNS username UNIQUE;


-- ================================================
-- Auth providers
-- Configured identity providers (bridges): Keycloak,
-- Auth0, Authentik, Google, Microsoft, LDAP, etc.
-- ================================================

DEFINE TABLE IF NOT EXISTS auth_providers;

DEFINE FIELD IF NOT EXISTS name ON TABLE auth_providers TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE auth_providers TYPE string;
-- "oidc" | "oauth2" | "saml" | "ldap" | "local"
DEFINE FIELD IF NOT EXISTS provider_type ON TABLE auth_providers TYPE string;
DEFINE FIELD IF NOT EXISTS issuer_url ON TABLE auth_providers TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS client_id ON TABLE auth_providers TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS client_secret_encrypted ON TABLE auth_providers TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS scopes ON TABLE auth_providers TYPE option<array<string> | NONE>;
-- Extra provider-specific settings (realm, tenant id, …)
DEFINE FIELD IF NOT EXISTS config ON TABLE auth_providers TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS enabled ON TABLE auth_providers TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE auth_providers TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE auth_providers TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS auth_providers_name ON TABLE auth_providers COLUMNS name UNIQUE;


-- ================================================
-- User identities
-- Links a core user to one or more external IdP accounts
-- (Google, Keycloak subject, Microsoft oid, …).
-- ================================================

DEFINE TABLE IF NOT EXISTS user_identities;

DEFINE FIELD IF NOT EXISTS user ON TABLE user_identities TYPE record<users>;
-- Matches auth_providers.name, or a well-known id like "local" / "google"
DEFINE FIELD IF NOT EXISTS provider ON TABLE user_identities TYPE string;
DEFINE FIELD IF NOT EXISTS provider_user_id ON TABLE user_identities TYPE string;
DEFINE FIELD IF NOT EXISTS email ON TABLE user_identities TYPE option<string | NONE>
    ASSERT $value = NONE OR string::is_email($value);
DEFINE FIELD IF NOT EXISTS access_token_encrypted ON TABLE user_identities TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS refresh_token_encrypted ON TABLE user_identities TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS token_expires_at ON TABLE user_identities TYPE option<datetime | NONE>;
-- Raw / normalized claims from the IdP (sub, groups, roles, …)
DEFINE FIELD IF NOT EXISTS claims ON TABLE user_identities TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE user_identities TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE user_identities TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS user_identities_provider_uid
    ON TABLE user_identities COLUMNS provider, provider_user_id UNIQUE;


-- ================================================
-- Permissions (platform catalog)
-- Keys like "user:read", "org:create", "plugin:install".
-- Superusers ignore these entirely.
-- ================================================

DEFINE TABLE IF NOT EXISTS permissions;

DEFINE FIELD IF NOT EXISTS s_key ON TABLE permissions TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE permissions TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE permissions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE permissions TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE permissions TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS permissions_key ON TABLE permissions COLUMNS s_key UNIQUE;


-- ================================================
-- Roles (platform)
-- ================================================

DEFINE TABLE IF NOT EXISTS roles;

DEFINE FIELD IF NOT EXISTS name ON TABLE roles TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE roles TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE roles TYPE option<string | NONE>;
-- System roles (e.g. "member") cannot be deleted by admins
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
-- Groups (platform)
-- Auth groups — not settings UI groups (gl_settings_groups).
-- Can map from Keycloak / IdP groups via user_identities.claims.
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


DEFINE TABLE IF NOT EXISTS group_members;

DEFINE FIELD IF NOT EXISTS group ON TABLE group_members TYPE record<groups>;
DEFINE FIELD IF NOT EXISTS user ON TABLE group_members TYPE record<users>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE group_members TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE group_members TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS group_members_unique
    ON TABLE group_members COLUMNS group, user UNIQUE;


-- Direct role assignment (in addition to roles via groups)
DEFINE TABLE IF NOT EXISTS user_roles;

DEFINE FIELD IF NOT EXISTS user ON TABLE user_roles TYPE record<users>;
DEFINE FIELD IF NOT EXISTS role ON TABLE user_roles TYPE record<roles>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE user_roles TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE user_roles TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS user_roles_unique
    ON TABLE user_roles COLUMNS user, role UNIQUE;


-- =========================================================================================
-- Org membership
-- Which orgs a user belongs to and whether they own that org.
-- Fine-grained org permissions live in the org DB (see org_migrations).
-- ==========================================================================================

DEFINE TABLE IF NOT EXISTS user_org_memberships;

DEFINE FIELD IF NOT EXISTS user ON TABLE user_org_memberships TYPE record<users>;
DEFINE FIELD IF NOT EXISTS org_database ON TABLE user_org_memberships TYPE record<org_databases>;
DEFINE FIELD IF NOT EXISTS is_org_owner ON TABLE user_org_memberships TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS is_active ON TABLE user_org_memberships TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE user_org_memberships TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE user_org_memberships TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS user_org_memberships_unique
    ON TABLE user_org_memberships COLUMNS user, org_database UNIQUE;

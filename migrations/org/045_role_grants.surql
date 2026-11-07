-- ================================================
-- Record access an administrator gave a custom role (see docs/architecture/role_grants.md)
--
-- One row per role and model: the operations the role may do to the model's records, in addition to what the
-- plugin's own `rules/<model>.json` grants. `model` is `<plugin>.<model>`. Grants only ever add access.
-- ================================================
DEFINE TABLE IF NOT EXISTS role_grants;

DEFINE FIELD IF NOT EXISTS role ON TABLE role_grants TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE role_grants TYPE string;
DEFINE FIELD IF NOT EXISTS operations ON TABLE role_grants TYPE array<string>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE role_grants TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE role_grants TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS role_grants_unique ON TABLE role_grants COLUMNS role, model UNIQUE;

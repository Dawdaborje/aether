-- Organization owner (one) vs administrators (many via org_admin role).
-- Ownership transfers are append-only history.

DEFINE FIELD IF NOT EXISTS owner_user_id ON TABLE organizations TYPE option<string | NONE>;

DEFINE TABLE IF NOT EXISTS org_ownership_transfers;
DEFINE FIELD IF NOT EXISTS organization_id ON TABLE org_ownership_transfers TYPE string;
DEFINE FIELD IF NOT EXISTS from_user_id ON TABLE org_ownership_transfers TYPE string;
DEFINE FIELD IF NOT EXISTS to_user_id ON TABLE org_ownership_transfers TYPE string;
DEFINE FIELD IF NOT EXISTS transferred_by ON TABLE org_ownership_transfers TYPE string;
DEFINE FIELD IF NOT EXISTS note ON TABLE org_ownership_transfers TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE org_ownership_transfers TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS org_ownership_transfers_org ON TABLE org_ownership_transfers COLUMNS organization_id;

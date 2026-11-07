-- ================================================
-- A person's own login at an outside service a bridge talks to for them (their mailbox on a JMAP server).
-- The secret is stored encrypted; a plugin never sees it, the bridge gets it from the kernel for the person calling.
-- ================================================

DEFINE TABLE IF NOT EXISTS user_bridge_credentials SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS user ON TABLE user_bridge_credentials TYPE string;
DEFINE FIELD IF NOT EXISTS bridge ON TABLE user_bridge_credentials TYPE string;
DEFINE FIELD IF NOT EXISTS username ON TABLE user_bridge_credentials TYPE string;
DEFINE FIELD IF NOT EXISTS secret ON TABLE user_bridge_credentials TYPE string;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE user_bridge_credentials TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS user_bridge_credentials_owner ON TABLE user_bridge_credentials FIELDS user, bridge UNIQUE;

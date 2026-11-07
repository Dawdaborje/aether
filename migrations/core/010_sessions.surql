-- ================================================
-- Sessions
-- Auth session / refresh-token state for core users.
-- Depends on users (005).
-- ================================================

DEFINE TABLE IF NOT EXISTS sessions;

DEFINE FIELD IF NOT EXISTS user ON TABLE sessions TYPE record<users>;
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE sessions TYPE string;
DEFINE FIELD IF NOT EXISTS refresh_token_hash ON TABLE sessions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS provider ON TABLE sessions TYPE option<string | NONE>;
-- Active org context for this session (org_databases record id string)
DEFINE FIELD IF NOT EXISTS org_database_id ON TABLE sessions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS ip_address ON TABLE sessions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS user_agent ON TABLE sessions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE sessions TYPE datetime;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE sessions TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE sessions TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE sessions TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS sessions_token_hash ON TABLE sessions COLUMNS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS sessions_refresh_token_hash ON TABLE sessions COLUMNS refresh_token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS sessions_user ON TABLE sessions COLUMNS user;

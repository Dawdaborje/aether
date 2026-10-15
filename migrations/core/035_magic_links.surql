-- Generic single-use magic links (auth reset/login and plugin purposes).
-- Rows are never hard-deleted: revoke sets revoked_at, consume sets used_at.

DEFINE TABLE IF NOT EXISTS magic_links;

DEFINE FIELD IF NOT EXISTS token_hash ON TABLE magic_links TYPE string;
DEFINE FIELD IF NOT EXISTS purpose ON TABLE magic_links TYPE string;
DEFINE FIELD IF NOT EXISTS user_id ON TABLE magic_links TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS email ON TABLE magic_links TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS payload ON TABLE magic_links TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE magic_links TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS org_db ON TABLE magic_links TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS path ON TABLE magic_links TYPE string DEFAULT "/magic";
DEFINE FIELD IF NOT EXISTS single_use ON TABLE magic_links TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE magic_links TYPE datetime;
DEFINE FIELD IF NOT EXISTS used_at ON TABLE magic_links TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE magic_links TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS created_by ON TABLE magic_links TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE magic_links TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE magic_links TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS magic_links_token_hash ON TABLE magic_links COLUMNS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS magic_links_purpose_user ON TABLE magic_links COLUMNS purpose, user_id;

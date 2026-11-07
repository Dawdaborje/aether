-- ================================================
-- API tokens: a credential for scripts and other systems (see docs/architecture/api_and_webhooks.md)
--
-- A token belongs to one person in one organization and can never do more than they can; `scopes`
-- narrows it further (which plugins and functions it may call, which models it may read or write
-- through the record API). Only the SHA-256 of the token is stored: the token itself is shown once,
-- when it is made. Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS api_tokens;
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE api_tokens TYPE string;
-- The first characters of the token, so a person can tell their tokens apart.
DEFINE FIELD IF NOT EXISTS prefix ON TABLE api_tokens TYPE string;
-- The owner, as `users:…`.
DEFINE FIELD IF NOT EXISTS user ON TABLE api_tokens TYPE string;
DEFINE FIELD IF NOT EXISTS org_db ON TABLE api_tokens TYPE string;
DEFINE FIELD IF NOT EXISTS name ON TABLE api_tokens TYPE string;
-- { plugins: [..], functions: ["plugin.function"], models: ["plugin.model:read,write"] }
DEFINE FIELD IF NOT EXISTS scopes ON TABLE api_tokens TYPE object;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE api_tokens TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE api_tokens TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_used_at ON TABLE api_tokens TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_ip ON TABLE api_tokens TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE api_tokens TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS api_tokens_hash ON TABLE api_tokens COLUMNS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS api_tokens_owner ON TABLE api_tokens COLUMNS org_db, user;

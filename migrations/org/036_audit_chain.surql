-- ================================================
-- Audit chain: making the audit trail tamper-evident (per organization)
--
-- Each audit table (data_access, plugin_calls, page_visits) is a chain. A background sealer gives
-- the rows not yet sealed a number (`chain_seq`, 1, 2, 3 ... per table) and links each to the one
-- before: `row_hash` is the SHA-256 of the row's canonical form, which includes `prev_hash`. Changing,
-- removing or reordering a sealed row breaks every link after it.
--
-- audit_chain_head   one row per chain: the last sealed `seq` and `hash`, the point verification
--                    starts from after a purge (`base_seq`, `base_hash`), and when the last
--                    checkpoint was made.
-- audit_checkpoints  a signed statement "up to `to_seq` the chain's hash was `head_hash`". The
--                    signature (Ed25519) is over the text in docs/architecture/audit_chain.md.
-- audit_keys         the public keys that signed checkpoints, so old ones still verify.
-- audit_verifications  the result of each verification run.
--
-- Written only by the kernel. See docs/architecture/audit_chain.md.
-- ================================================

DEFINE FIELD IF NOT EXISTS chain_seq ON TABLE data_access TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS prev_hash ON TABLE data_access TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS row_hash ON TABLE data_access TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS data_access_chain ON TABLE data_access COLUMNS chain_seq;

DEFINE FIELD IF NOT EXISTS chain_seq ON TABLE plugin_calls TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS prev_hash ON TABLE plugin_calls TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS row_hash ON TABLE plugin_calls TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS plugin_calls_chain ON TABLE plugin_calls COLUMNS chain_seq;

DEFINE FIELD IF NOT EXISTS chain_seq ON TABLE page_visits TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS prev_hash ON TABLE page_visits TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS row_hash ON TABLE page_visits TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS page_visits_chain ON TABLE page_visits COLUMNS chain_seq;

DEFINE TABLE IF NOT EXISTS audit_chain_head;
-- "data_access" | "plugin_calls" | "page_visits"
DEFINE FIELD IF NOT EXISTS chain ON TABLE audit_chain_head TYPE string;
DEFINE FIELD IF NOT EXISTS seq ON TABLE audit_chain_head TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS hash ON TABLE audit_chain_head TYPE string DEFAULT '';
-- Rows up to `base_seq` were purged; the first kept row links to `base_hash`.
DEFINE FIELD IF NOT EXISTS base_seq ON TABLE audit_chain_head TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS base_hash ON TABLE audit_chain_head TYPE string DEFAULT '';
DEFINE FIELD IF NOT EXISTS checkpoint_seq ON TABLE audit_chain_head TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS checkpoint_ns ON TABLE audit_chain_head TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS started_ns ON TABLE audit_chain_head TYPE int DEFAULT 0;
DEFINE INDEX IF NOT EXISTS audit_chain_head_chain ON TABLE audit_chain_head COLUMNS chain UNIQUE;

DEFINE TABLE IF NOT EXISTS audit_checkpoints;
DEFINE FIELD IF NOT EXISTS chain ON TABLE audit_checkpoints TYPE string;
DEFINE FIELD IF NOT EXISTS org ON TABLE audit_checkpoints TYPE string;
DEFINE FIELD IF NOT EXISTS from_seq ON TABLE audit_checkpoints TYPE int;
DEFINE FIELD IF NOT EXISTS to_seq ON TABLE audit_checkpoints TYPE int;
DEFINE FIELD IF NOT EXISTS head_hash ON TABLE audit_checkpoints TYPE string;
-- When it was made, in nanoseconds since the epoch (what the signature covers).
DEFINE FIELD IF NOT EXISTS at_ns ON TABLE audit_checkpoints TYPE int;
DEFINE FIELD IF NOT EXISTS key_id ON TABLE audit_checkpoints TYPE string;
DEFINE FIELD IF NOT EXISTS signature ON TABLE audit_checkpoints TYPE string;
DEFINE INDEX IF NOT EXISTS audit_checkpoints_chain ON TABLE audit_checkpoints COLUMNS chain, to_seq UNIQUE;

DEFINE TABLE IF NOT EXISTS audit_keys;
DEFINE FIELD IF NOT EXISTS key_id ON TABLE audit_keys TYPE string;
-- Hex of the 32-byte Ed25519 public key.
DEFINE FIELD IF NOT EXISTS public_key ON TABLE audit_keys TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE audit_keys TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS audit_keys_id ON TABLE audit_keys COLUMNS key_id UNIQUE;

DEFINE TABLE IF NOT EXISTS audit_verifications;
DEFINE FIELD IF NOT EXISTS ok ON TABLE audit_verifications TYPE bool;
DEFINE FIELD IF NOT EXISTS report ON TABLE audit_verifications TYPE object;
DEFINE FIELD IF NOT EXISTS at ON TABLE audit_verifications TYPE datetime DEFAULT time::now();

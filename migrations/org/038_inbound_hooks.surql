-- ================================================
-- Inbound hooks: addresses outside systems call to tell a plugin something
-- (see docs/architecture/api_and_webhooks.md)
--
-- One row per hook a plugin declares (`[[inbound]]`) that an administrator has made a secret for. The
-- secret is what the other system signs with; it is stored encrypted. Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS inbound_hooks;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE inbound_hooks TYPE string;
DEFINE FIELD IF NOT EXISTS name ON TABLE inbound_hooks TYPE string;
DEFINE FIELD IF NOT EXISTS secret ON TABLE inbound_hooks TYPE string;
DEFINE FIELD IF NOT EXISTS secret_prev ON TABLE inbound_hooks TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS secret_prev_until ON TABLE inbound_hooks TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS enabled ON TABLE inbound_hooks TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS received ON TABLE inbound_hooks TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS rejected ON TABLE inbound_hooks TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS last_received_at ON TABLE inbound_hooks TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_rejected_at ON TABLE inbound_hooks TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_rejected_reason ON TABLE inbound_hooks TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS created_by ON TABLE inbound_hooks TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE inbound_hooks TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS inbound_hooks_hook ON TABLE inbound_hooks COLUMNS plugin, name UNIQUE;

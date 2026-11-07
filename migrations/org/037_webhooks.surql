-- ================================================
-- Webhooks: telling outside systems that something happened (see docs/architecture/api_and_webhooks.md)
--
-- webhook_subscriptions  where an organization wants some events sent: an address, the events, an
--                        optional equality filter, and the signing secret (encrypted). Managed by
--                        an administrator; written only by the kernel.
-- webhook_deliveries     every attempt to send one event to one subscription: what was sent, how it
--                        went, and when to try again. Kept for `webhooks.log_days`, and can be sent again.
-- ================================================

DEFINE TABLE IF NOT EXISTS webhook_subscriptions;
DEFINE FIELD IF NOT EXISTS name ON TABLE webhook_subscriptions TYPE string;
DEFINE FIELD IF NOT EXISTS url ON TABLE webhook_subscriptions TYPE string;
-- Full event names, `<plugin>.<event>`.
DEFINE FIELD IF NOT EXISTS events ON TABLE webhook_subscriptions TYPE array<string>;
-- Equality conditions on the sent fields; every one must hold: { "status": "won" }.
DEFINE FIELD IF NOT EXISTS conditions ON TABLE webhook_subscriptions TYPE option<object | NONE>;
-- `enc:v1:…`, see secrets.rs.
DEFINE FIELD IF NOT EXISTS secret ON TABLE webhook_subscriptions TYPE string;
-- The secret before the last rotation, still signed with until `secret_prev_until`.
DEFINE FIELD IF NOT EXISTS secret_prev ON TABLE webhook_subscriptions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS secret_prev_until ON TABLE webhook_subscriptions TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS active ON TABLE webhook_subscriptions TYPE bool DEFAULT true;
-- Deliveries in a row that gave up; the subscription is switched off past `webhooks.disable_after`.
DEFINE FIELD IF NOT EXISTS failures ON TABLE webhook_subscriptions TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS disabled_reason ON TABLE webhook_subscriptions TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS created_by ON TABLE webhook_subscriptions TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE webhook_subscriptions TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS webhook_subscriptions_active ON TABLE webhook_subscriptions COLUMNS active;

DEFINE TABLE IF NOT EXISTS webhook_deliveries;
DEFINE FIELD IF NOT EXISTS ulid ON TABLE webhook_deliveries TYPE string;
DEFINE FIELD IF NOT EXISTS subscription ON TABLE webhook_deliveries TYPE string;
DEFINE FIELD IF NOT EXISTS event ON TABLE webhook_deliveries TYPE string;
-- The data sent (already cut down to the fields the event allows).
DEFINE FIELD IF NOT EXISTS data ON TABLE webhook_deliveries TYPE option<object | NONE>;
-- "pending" | "retrying" | "delivered" | "dead"
DEFINE FIELD IF NOT EXISTS status ON TABLE webhook_deliveries TYPE string;
DEFINE FIELD IF NOT EXISTS attempts ON TABLE webhook_deliveries TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS http_status ON TABLE webhook_deliveries TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS error ON TABLE webhook_deliveries TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS response_head ON TABLE webhook_deliveries TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS duration_ms ON TABLE webhook_deliveries TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS replay_of ON TABLE webhook_deliveries TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE webhook_deliveries TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS last_attempt_at ON TABLE webhook_deliveries TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS webhook_deliveries_ulid ON TABLE webhook_deliveries COLUMNS ulid UNIQUE;
DEFINE INDEX IF NOT EXISTS webhook_deliveries_subscription ON TABLE webhook_deliveries COLUMNS subscription, created_at;
DEFINE INDEX IF NOT EXISTS webhook_deliveries_created ON TABLE webhook_deliveries COLUMNS created_at;

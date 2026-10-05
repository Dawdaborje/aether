-- ================================================
-- Event subscriptions (per organization)
--
-- Which plugin function handles which event. A plugin that emits an event (`events::emit`) makes a
-- background job for every subscriber installed and enabled in the organization. Rows come from the
-- plugin's manifest (source "manifest", kept in step with it) or from `events::subscribe` while the
-- plugin runs (source "runtime"). Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS event_subscriptions;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE event_subscriptions TYPE string;
-- "<emitting plugin>.<event>", such as "chat.message_posted".
DEFINE FIELD IF NOT EXISTS event ON TABLE event_subscriptions TYPE string;
DEFINE FIELD IF NOT EXISTS function_name ON TABLE event_subscriptions TYPE string;
DEFINE FIELD IF NOT EXISTS source ON TABLE event_subscriptions TYPE string DEFAULT 'manifest';
DEFINE FIELD IF NOT EXISTS queue ON TABLE event_subscriptions TYPE string DEFAULT 'default';
DEFINE FIELD IF NOT EXISTS max_attempts ON TABLE event_subscriptions TYPE int DEFAULT 3;
DEFINE FIELD IF NOT EXISTS enabled ON TABLE event_subscriptions TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE event_subscriptions TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS event_subscriptions_unique ON TABLE event_subscriptions COLUMNS plugin, event UNIQUE;
DEFINE INDEX IF NOT EXISTS event_subscriptions_event ON TABLE event_subscriptions COLUMNS event, enabled;

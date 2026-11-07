-- The events a plugin listens to (`[[events]]` with `direction = "listen"` in its manifest):
-- { event: "chat.message_posted", function, queue, max_attempts }. Installing the plugin in an
-- organization copies them into that organization's `event_subscriptions`.
DEFINE FIELD IF NOT EXISTS event_listeners ON TABLE plugins TYPE array<object> DEFAULT [];

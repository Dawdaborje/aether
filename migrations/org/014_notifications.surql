-- ================================================
-- Notifications (per organization)
--
-- notifications       what the kernel (or a plugin through the kernel) told people.
--                     One row per notification, whoever it is for: `audience_kind`
--                     says who may see it, so a broadcast to everyone is one row.
-- notification_reads  which actor has read which notification.
--
-- `ulid` is time-ordered and unique; it is the id sent over the event stream, so
-- a reconnecting browser asks for everything after the last one it saw.
-- Written only by the kernel; plugins reach it through the `notify::send`
-- capability and cannot name these tables as models.
-- ================================================

DEFINE TABLE IF NOT EXISTS notifications;
DEFINE FIELD IF NOT EXISTS ulid ON TABLE notifications TYPE string;
-- "members" (logged-in members) | "everyone" (members and visitors) | "actors"
DEFINE FIELD IF NOT EXISTS audience_kind ON TABLE notifications TYPE string;
-- Actor ids (users:… or visitors:…) when audience_kind is "actors".
DEFINE FIELD IF NOT EXISTS audience_actors ON TABLE notifications TYPE array<string> DEFAULT [];
-- "kernel" or the name of the plugin that sent it.
DEFINE FIELD IF NOT EXISTS source ON TABLE notifications TYPE string;
-- "info" | "success" | "warning" | "error"
DEFINE FIELD IF NOT EXISTS level ON TABLE notifications TYPE string DEFAULT 'info';
DEFINE FIELD IF NOT EXISTS title ON TABLE notifications TYPE string;
DEFINE FIELD IF NOT EXISTS body ON TABLE notifications TYPE option<string | NONE>;
-- An app path to open when the notification is clicked.
DEFINE FIELD IF NOT EXISTS link ON TABLE notifications TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS payload ON TABLE notifications TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE notifications TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE notifications TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS notifications_ulid ON TABLE notifications COLUMNS ulid UNIQUE;
DEFINE INDEX IF NOT EXISTS notifications_created ON TABLE notifications COLUMNS date_created;

DEFINE TABLE IF NOT EXISTS notification_reads;
DEFINE FIELD IF NOT EXISTS notification ON TABLE notification_reads TYPE string;
DEFINE FIELD IF NOT EXISTS actor ON TABLE notification_reads TYPE string;
DEFINE FIELD IF NOT EXISTS read_at ON TABLE notification_reads TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS notification_reads_unique ON TABLE notification_reads COLUMNS notification, actor UNIQUE;
DEFINE INDEX IF NOT EXISTS notification_reads_actor ON TABLE notification_reads COLUMNS actor;

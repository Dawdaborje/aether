-- ================================================
-- Chatter: the conversation and history of a record (per organization)
--
-- chatter_messages   messages, internal notes, system lines and recorded field changes of
--                    a record, keyed by (model_id, record_key). Nothing is ever deleted by a
--                    user: `deleted_at` puts a message in the trash, where it stays until the
--                    retention setting (`chatter.trash_retention_days`) lets cleanup remove it.
-- chatter_followers  who follows a record, and whether they muted it.
--
-- Written only by the kernel. Plugins cannot name these tables as models.
-- ================================================

DEFINE TABLE IF NOT EXISTS chatter_messages;
DEFINE FIELD IF NOT EXISTS ulid ON TABLE chatter_messages TYPE string;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE chatter_messages TYPE string;
DEFINE FIELD IF NOT EXISTS record_key ON TABLE chatter_messages TYPE string;
-- "message" (notifies followers) | "note" (internal) | "system" | "change"
DEFINE FIELD IF NOT EXISTS kind ON TABLE chatter_messages TYPE string;
-- users:… or visitors:…, or "system" for a line the kernel or a plugin wrote
DEFINE FIELD IF NOT EXISTS author ON TABLE chatter_messages TYPE string;
-- The name to show when it cannot be looked up (a visitor's own name).
DEFINE FIELD IF NOT EXISTS author_name ON TABLE chatter_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS body ON TABLE chatter_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS mentions ON TABLE chatter_messages TYPE array<string> DEFAULT [];
-- An app path to the page of the record.
DEFINE FIELD IF NOT EXISTS link ON TABLE chatter_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE chatter_messages TYPE option<string | NONE>;
-- For a change: the tracked fields before and after, by field id.
DEFINE FIELD IF NOT EXISTS before ON TABLE chatter_messages TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS after ON TABLE chatter_messages TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE chatter_messages TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS edited_at ON TABLE chatter_messages TYPE option<datetime | NONE>;
-- Earlier texts of an edited message: [{ body, at }]
DEFINE FIELD IF NOT EXISTS edits ON TABLE chatter_messages TYPE array<object> DEFAULT [];
DEFINE FIELD IF NOT EXISTS deleted_at ON TABLE chatter_messages TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS deleted_by ON TABLE chatter_messages TYPE option<string | NONE>;
-- Set when the record itself was deleted: the whole thread then waits in the trash.
DEFINE FIELD IF NOT EXISTS record_deleted_at ON TABLE chatter_messages TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS chatter_messages_ulid ON TABLE chatter_messages COLUMNS ulid UNIQUE;
DEFINE INDEX IF NOT EXISTS chatter_messages_thread ON TABLE chatter_messages COLUMNS model_id, record_key, ulid;
DEFINE INDEX IF NOT EXISTS chatter_messages_deleted ON TABLE chatter_messages COLUMNS deleted_at;

DEFINE TABLE IF NOT EXISTS chatter_followers;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE chatter_followers TYPE string;
DEFINE FIELD IF NOT EXISTS record_key ON TABLE chatter_followers TYPE string;
DEFINE FIELD IF NOT EXISTS actor ON TABLE chatter_followers TYPE string;
-- "creator" | "commenter" | "mention" | "manual"
DEFINE FIELD IF NOT EXISTS reason ON TABLE chatter_followers TYPE string;
DEFINE FIELD IF NOT EXISTS muted ON TABLE chatter_followers TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE chatter_followers TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS chatter_followers_unique ON TABLE chatter_followers COLUMNS model_id, record_key, actor UNIQUE;
DEFINE INDEX IF NOT EXISTS chatter_followers_actor ON TABLE chatter_followers COLUMNS actor;

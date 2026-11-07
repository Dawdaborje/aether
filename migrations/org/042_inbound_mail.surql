-- ================================================
-- Inbound mail (see docs/architecture/inbound_mail.md)
--
-- `mail_inbox`: one row, the organization's inbound address: the secret a mail provider proves itself with
--   (stored encrypted) and counters. `inbound_messages`: every message accepted, with what became of it;
--   its `message_id` and `alias` are unique so a provider's retry creates nothing twice. `mail_threads`: the
--   short token that ties an address or a Message-ID to one record. Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS mail_inbox;
DEFINE FIELD IF NOT EXISTS secret ON TABLE mail_inbox TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS secret_prev ON TABLE mail_inbox TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS secret_prev_until ON TABLE mail_inbox TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS enabled ON TABLE mail_inbox TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS received ON TABLE mail_inbox TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS rejected ON TABLE mail_inbox TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS last_received_at ON TABLE mail_inbox TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_rejected_at ON TABLE mail_inbox TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS last_rejected_reason ON TABLE mail_inbox TYPE option<string | NONE>;

DEFINE TABLE IF NOT EXISTS inbound_messages;
DEFINE FIELD IF NOT EXISTS message_id ON TABLE inbound_messages TYPE string;
DEFINE FIELD IF NOT EXISTS alias ON TABLE inbound_messages TYPE string;
DEFINE FIELD IF NOT EXISTS sender ON TABLE inbound_messages TYPE string;
DEFINE FIELD IF NOT EXISTS subject ON TABLE inbound_messages TYPE string;
-- received | routed | replied | unmatched | ignored | failed
DEFINE FIELD IF NOT EXISTS status ON TABLE inbound_messages TYPE string;
DEFINE FIELD IF NOT EXISTS reason ON TABLE inbound_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE inbound_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS route_function ON TABLE inbound_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE inbound_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS record_key ON TABLE inbound_messages TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS message ON TABLE inbound_messages TYPE object;
DEFINE FIELD IF NOT EXISTS received_at ON TABLE inbound_messages TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS handled_at ON TABLE inbound_messages TYPE option<datetime | NONE>;
DEFINE INDEX IF NOT EXISTS inbound_messages_once ON TABLE inbound_messages COLUMNS message_id, alias UNIQUE;
DEFINE INDEX IF NOT EXISTS inbound_messages_status ON TABLE inbound_messages COLUMNS status, received_at;
DEFINE INDEX IF NOT EXISTS inbound_messages_record ON TABLE inbound_messages COLUMNS model_id, record_key;

DEFINE TABLE IF NOT EXISTS mail_threads;
DEFINE FIELD IF NOT EXISTS token ON TABLE mail_threads TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE mail_threads TYPE string;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE mail_threads TYPE string;
DEFINE FIELD IF NOT EXISTS record_key ON TABLE mail_threads TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE mail_threads TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS mail_threads_token ON TABLE mail_threads COLUMNS token UNIQUE;
DEFINE INDEX IF NOT EXISTS mail_threads_record ON TABLE mail_threads COLUMNS model_id, record_key, plugin UNIQUE;

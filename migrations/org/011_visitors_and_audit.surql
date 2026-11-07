-- ================================================
-- Visitors and the access audit trail (per organization)
--
-- visitors      anonymous identities, one per browser (cookie), optionally
--               linked to a user once they log in.
-- page_visits   who opened which page.
-- plugin_calls  who called which plugin function.
-- data_access   every database access a plugin made, with record ids.
--
-- All four are written only by the kernel. Plugins cannot name these tables
-- as models, and raw SurQL that mentions them is rejected.
-- ================================================

DEFINE TABLE IF NOT EXISTS visitors;
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE visitors TYPE string;
DEFINE FIELD IF NOT EXISTS first_seen ON TABLE visitors TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS last_seen ON TABLE visitors TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS ip ON TABLE visitors TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS user_agent ON TABLE visitors TYPE option<string | NONE>;
-- Core user id string (users:xxx) once this browser has logged in.
DEFINE FIELD IF NOT EXISTS linked_user ON TABLE visitors TYPE option<string | NONE>;
DEFINE INDEX IF NOT EXISTS visitors_token_hash ON TABLE visitors COLUMNS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS visitors_last_seen ON TABLE visitors COLUMNS last_seen;
DEFINE INDEX IF NOT EXISTS visitors_linked_user ON TABLE visitors COLUMNS linked_user;

DEFINE TABLE IF NOT EXISTS page_visits;
DEFINE FIELD IF NOT EXISTS request_id ON TABLE page_visits TYPE string;
-- "user" | "visitor" | "anonymous" (no visitor identity: denied before one was issued)
DEFINE FIELD IF NOT EXISTS actor_type ON TABLE page_visits TYPE string;
DEFINE FIELD IF NOT EXISTS actor_id ON TABLE page_visits TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE page_visits TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS route ON TABLE page_visits TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS path ON TABLE page_visits TYPE string;
DEFINE FIELD IF NOT EXISTS method ON TABLE page_visits TYPE string;
DEFINE FIELD IF NOT EXISTS status ON TABLE page_visits TYPE int;
DEFINE FIELD IF NOT EXISTS ip ON TABLE page_visits TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS user_agent ON TABLE page_visits TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE page_visits TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS page_visits_request ON TABLE page_visits COLUMNS request_id;
DEFINE INDEX IF NOT EXISTS page_visits_actor ON TABLE page_visits COLUMNS actor_type, actor_id;
DEFINE INDEX IF NOT EXISTS page_visits_route ON TABLE page_visits COLUMNS route;
DEFINE INDEX IF NOT EXISTS page_visits_date ON TABLE page_visits COLUMNS date_created;

DEFINE TABLE IF NOT EXISTS plugin_calls;
DEFINE FIELD IF NOT EXISTS request_id ON TABLE plugin_calls TYPE string;
DEFINE FIELD IF NOT EXISTS actor_type ON TABLE plugin_calls TYPE string;
DEFINE FIELD IF NOT EXISTS actor_id ON TABLE plugin_calls TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE plugin_calls TYPE string;
DEFINE FIELD IF NOT EXISTS function_name ON TABLE plugin_calls TYPE string;
DEFINE FIELD IF NOT EXISTS status ON TABLE plugin_calls TYPE int;
DEFINE FIELD IF NOT EXISTS ip ON TABLE plugin_calls TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS user_agent ON TABLE plugin_calls TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE plugin_calls TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS plugin_calls_request ON TABLE plugin_calls COLUMNS request_id;
DEFINE INDEX IF NOT EXISTS plugin_calls_actor ON TABLE plugin_calls COLUMNS actor_type, actor_id;
DEFINE INDEX IF NOT EXISTS plugin_calls_date ON TABLE plugin_calls COLUMNS date_created;

DEFINE TABLE IF NOT EXISTS data_access;
DEFINE FIELD IF NOT EXISTS request_id ON TABLE data_access TYPE string;
DEFINE FIELD IF NOT EXISTS actor_type ON TABLE data_access TYPE string;
DEFINE FIELD IF NOT EXISTS actor_id ON TABLE data_access TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE data_access TYPE string;
DEFINE FIELD IF NOT EXISTS function_name ON TABLE data_access TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE data_access TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS table_name ON TABLE data_access TYPE option<string | NONE>;
-- "read" | "create" | "update" | "delete" | "raw"
DEFINE FIELD IF NOT EXISTS operation ON TABLE data_access TYPE string;
DEFINE FIELD IF NOT EXISTS record_ids ON TABLE data_access TYPE array<record> DEFAULT [];
DEFINE FIELD IF NOT EXISTS record_count ON TABLE data_access TYPE int DEFAULT 0;
-- Raw SurQL text (operation = "raw"); record ids are unknown for these.
DEFINE FIELD IF NOT EXISTS statement ON TABLE data_access TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS ip ON TABLE data_access TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE data_access TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS data_access_request ON TABLE data_access COLUMNS request_id;
DEFINE INDEX IF NOT EXISTS data_access_actor ON TABLE data_access COLUMNS actor_type, actor_id;
DEFINE INDEX IF NOT EXISTS data_access_table ON TABLE data_access COLUMNS table_name;
DEFINE INDEX IF NOT EXISTS data_access_date ON TABLE data_access COLUMNS date_created;

-- Per-user working defaults inside an organization (current company, …).
-- Masters such as party stay shared; this only stores shell / document context.

DEFINE TABLE IF NOT EXISTS aether_user_defaults SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS user_id ON TABLE aether_user_defaults TYPE string;
DEFINE FIELD IF NOT EXISTS company_id ON TABLE aether_user_defaults TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE aether_user_defaults TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE aether_user_defaults TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS aether_user_defaults_user ON TABLE aether_user_defaults COLUMNS user_id UNIQUE;

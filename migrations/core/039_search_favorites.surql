-- ================================================
-- Per-user saved list searches (faceted search favorites).
-- ================================================

DEFINE TABLE IF NOT EXISTS search_favorites SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS user_id ON TABLE search_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE search_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE search_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS filter ON TABLE search_favorites TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS q ON TABLE search_favorites TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS group_by ON TABLE search_favorites TYPE array<string>;
DEFINE FIELD IF NOT EXISTS is_default ON TABLE search_favorites TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE search_favorites TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE search_favorites TYPE datetime DEFAULT time::now();

DEFINE INDEX IF NOT EXISTS search_favorites_user_model ON TABLE search_favorites COLUMNS user_id, model;

-- ================================================
-- Records a person has starred (the Favorite button on a record's page and the Favorites filter on lists).
-- One row per person and record; the row key is `<user>|<plugin>|<model>|<record>`, so starring twice changes nothing.
-- ================================================

DEFINE TABLE IF NOT EXISTS record_favorites SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS user_id ON TABLE record_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE record_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS model ON TABLE record_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS record_id ON TABLE record_favorites TYPE string;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE record_favorites TYPE datetime DEFAULT time::now();

DEFINE INDEX IF NOT EXISTS record_favorites_user_model ON TABLE record_favorites COLUMNS user_id, plugin, model;

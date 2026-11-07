-- ================================================
-- UI themes
-- Theme definitions available inside this org
-- (tokens, fonts, surfaces).
-- ================================================

DEFINE TABLE IF NOT EXISTS ui_themes;

DEFINE FIELD IF NOT EXISTS name ON TABLE ui_themes TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE ui_themes TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE ui_themes TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_system ON TABLE ui_themes TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS is_active ON TABLE ui_themes TYPE bool DEFAULT true;
-- Design tokens: colors, radii, fonts, etc.
DEFINE FIELD IF NOT EXISTS tokens ON TABLE ui_themes TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS preview_image ON TABLE ui_themes TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE ui_themes TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE ui_themes TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS ui_themes_name ON TABLE ui_themes COLUMNS name UNIQUE;

-- =====================================================
-- SETTINGS GROUPS
-- =====================================================

DEFINE TABLE IF NOT EXISTS gl_settings_groups;

DEFINE FIELD IF NOT EXISTS label ON TABLE gl_settings_groups TYPE string;
DEFINE FIELD IF NOT EXISTS color ON TABLE gl_settings_groups TYPE option<string | NONE> DEFAULT "black";
DEFINE FIELD IF NOT EXISTS icon ON TABLE gl_settings_groups TYPE option<string | NONE> DEFAULT NONE;
DEFINE FIELD IF NOT EXISTS icon_type ON TABLE gl_settings_groups TYPE option<string | NONE> DEFAULT NONE;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE gl_settings_groups TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE gl_settings_groups TYPE datetime VALUE time::now();


DEFINE INDEX IF NOT EXISTS gl_setting_group_label
ON TABLE gl_settings_groups COLUMNS label UNIQUE;


-- =====================================================
-- SETTINGS ITEMS
-- =====================================================

DEFINE TABLE IF NOT EXISTS gl_settings_items;

DEFINE FIELD IF NOT EXISTS label ON TABLE gl_settings_items TYPE string;
DEFINE FIELD IF NOT EXISTS s_key ON TABLE gl_settings_items TYPE string; -- key is a keyword in the SurrealQL so I use s_key (s stands for "Setting", as in "Setting key" and not the SurrealDB "KEY")
DEFINE FIELD IF NOT EXISTS s_value ON TABLE gl_settings_items; -- s_value (s stands for "Setting", as in "Setting value" and not the SurrealDB "VALUE")
DEFINE FIELD IF NOT EXISTS description ON TABLE gl_settings_items TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS long_description ON TABLE gl_settings_items TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE gl_settings_items TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE gl_settings_items TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS gl_setting_items_label ON TABLE gl_settings_items COLUMNS label UNIQUE;
DEFINE INDEX IF NOT EXISTS gl_setting_items_key ON TABLE gl_settings_items COLUMNS key UNIQUE;


-- =====================================================
-- GROUP ↔ ITEM RELATION TABLE
-- =====================================================

DEFINE TABLE IF NOT EXISTS gl_settings_group_items;

DEFINE FIELD IF NOT EXISTS group ON TABLE gl_settings_group_items TYPE record<gl_settings_groups>;
DEFINE FIELD IF NOT EXISTS item ON TABLE gl_settings_group_items TYPE record<gl_settings_items>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE gl_settings_group_items TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE gl_settings_group_items TYPE datetime VALUE time::now();
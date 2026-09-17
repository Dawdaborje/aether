-- =====================================================
-- SETTINGS GROUPS
-- This doesn't run on the main database rather it runs
-- =====================================================

DEFINE TABLE IF NOT EXISTS settings_groups;

DEFINE FIELD IF NOT EXISTS label ON TABLE settings_groups TYPE string;
DEFINE FIELD IF NOT EXISTS color ON TABLE settings_groups TYPE option<string | NONE> DEFAULT "black";
DEFINE FIELD IF NOT EXISTS icon ON TABLE settings_groups TYPE option<string | NONE> DEFAULT NONE;
DEFINE FIELD IF NOT EXISTS icon_type ON TABLE settings_groups TYPE option<"svg" | "iconify" > DEFAULT "iconify";
DEFINE FIELD IF NOT EXISTS date_created ON TABLE settings_groups TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE settings_groups TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS setting_group_label
ON TABLE settings_groups COLUMNS label UNIQUE;


-- =====================================================
-- SETTINGS ITEMS
-- =====================================================

DEFINE TABLE IF NOT EXISTS settings_items;

DEFINE FIELD IF NOT EXISTS label ON TABLE settings_items TYPE string;
DEFINE FIELD IF NOT EXISTS s_key ON TABLE settings_items TYPE string; -- key is a keyword in the SurrealQL so I use s_key (s stands for "Setting", as in "Setting key" and not the SurrealDB "KEY")
DEFINE FIELD IF NOT EXISTS s_value ON TABLE settings_items; -- s_value (s stands for "Setting", as in "Setting value" and not the SurrealDB "VALUE". And the value can be dynamic and will be cast to the appropriate type by the backend)
DEFINE FIELD IF NOT EXISTS description ON TABLE settings_items TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS long_description ON TABLE settings_items TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE settings_items TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE settings_items TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS setting_items_label ON TABLE settings_items COLUMNS label UNIQUE;
DEFINE INDEX IF NOT EXISTS setting_items_key ON TABLE settings_items COLUMNS s_key UNIQUE;


-- =====================================================
-- GROUP ↔ ITEM RELATION TABLE
-- =====================================================

DEFINE TABLE IF NOT EXISTS settings_group_items;

DEFINE FIELD IF NOT EXISTS group ON TABLE settings_group_items TYPE record<settings_groups>;
DEFINE FIELD IF NOT EXISTS item ON TABLE settings_group_items TYPE record<settings_items>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE settings_group_items TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE settings_group_items TYPE datetime VALUE time::now();
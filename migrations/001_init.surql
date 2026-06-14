/* =========================
   SETTINGS SYSTEM SCHEMA
   Tabs → Groups → Items
   SurrealDB Full Graph Model
========================= */

/* -------------------------
   TABLE: setting_tabs
------------------------- */
DEFINE TABLE setting_tabs SCHEMAFULL;

DEFINE FIELD name       ON TABLE setting_tabs TYPE string;
DEFINE FIELD link       ON TABLE setting_tabs TYPE string;
DEFINE FIELD icon       ON TABLE setting_tabs TYPE string;
DEFINE FIELD icon_type  ON TABLE setting_tabs TYPE string;
DEFINE FIELD is_default ON TABLE setting_tabs TYPE bool DEFAULT false;

DEFINE FIELD created_at ON TABLE setting_tabs TYPE datetime VALUE time::now();
DEFINE FIELD updated_at ON TABLE setting_tabs TYPE datetime VALUE time::now();


/* Ensure only ONE default tab exists */
DEFINE EVENT ensure_single_default_tab ON TABLE setting_tabs
WHEN $after.is_default = true
THEN (
    UPDATE setting_tabs
    SET is_default = false
    WHERE id != $after.id AND is_default = true
);



/* -------------------------
   TABLE: setting_groups
------------------------- */
DEFINE TABLE setting_groups SCHEMAFULL;

DEFINE FIELD name ON TABLE setting_groups TYPE string;

DEFINE FIELD created_at ON TABLE setting_groups TYPE datetime VALUE time::now();
DEFINE FIELD updated_at ON TABLE setting_groups TYPE datetime VALUE time::now();



/* -------------------------
   TABLE: setting_items
------------------------- */
DEFINE TABLE setting_items SCHEMAFULL;

DEFINE FIELD key   ON TABLE setting_items TYPE string;
DEFINE FIELD label ON TABLE setting_items TYPE string;

DEFINE FIELD type ON TABLE setting_items TYPE string
ASSERT $value IN ["text", "toggle", "select", "number", "password"];

DEFINE FIELD value ON TABLE setting_items TYPE any;

DEFINE FIELD created_at ON TABLE setting_items TYPE datetime VALUE time::now();
DEFINE FIELD updated_at ON TABLE setting_items TYPE datetime VALUE time::now();



/* -------------------------
   RELATION: TAB → GROUPS
------------------------- */
DEFINE TABLE has_group TYPE RELATION;

DEFINE FIELD in  ON TABLE has_group TYPE record<setting_tabs>;
DEFINE FIELD out ON TABLE has_group TYPE record<setting_groups>;



/* -------------------------
   RELATION: GROUP → ITEMS
------------------------- */
DEFINE TABLE has_item TYPE RELATION;

DEFINE FIELD in  ON TABLE has_item TYPE record<setting_groups>;
DEFINE FIELD out ON TABLE has_item TYPE record<setting_items>;

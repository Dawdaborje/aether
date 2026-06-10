DEFINE TABLE setting_tabs;
DEFINE TABLE setting_groups;
DEFINE TABLE setting_items;

-- field definitions for setting_tabs
DEFINE FIELD name on TABLE setting_tabs TYPE string;
DEFINE FIELD link on TABLE setting_tabs TYPE string;
DEFINE FIELD icon on TABLE setting_tabs TYPE string;
DEFINE FIELD icon_type on TABLE setting_tabs TYPE string;
DEFINE  FIELD  is_default on TABLE setting_tabs TYPE string;

DEFINE INDEX is_default_unique ON TABLE setting_tabs COLUMNS is_default UNIQUE;


-- field definitions for setting_groups
DEFINE FIELD name on TABLE setting_groups TYPE string;


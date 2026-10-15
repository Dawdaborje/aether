-- Roles required to open a page / see it in app menus (empty = any member).
-- Names are stored qualified (`hr.hr_user`) when the plugin is loaded.

DEFINE FIELD IF NOT EXISTS required_roles ON TABLE plugin_ui_pages TYPE array<string> DEFAULT [];

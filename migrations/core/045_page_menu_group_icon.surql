-- Lucide icon for a page's collapsible sidebar section (`menu_group_icon` on `<page>`).
DEFINE FIELD IF NOT EXISTS menu_group_icon ON TABLE plugin_ui_pages TYPE option<string | NONE>;

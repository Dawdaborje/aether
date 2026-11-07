-- Page content layouts: wrap= names a component whose default <slot /> receives
-- the page body (SvelteKit-style children). Separate from shell chrome `layout`.

DEFINE FIELD IF NOT EXISTS wrap ON TABLE plugin_ui_pages TYPE option<string | NONE>;

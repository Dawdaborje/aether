-- ================================================
-- Parameterised page routes
--
-- A page route may contain `{param}` segments (`/chat/{channel}`).
--   is_pattern   the route has parameters.
--   route_shape  the route with each parameter written `{}`; two pages with
--                the same shape match the same URLs, so installing both in
--                one organization is a conflict.
-- ================================================

DEFINE FIELD IF NOT EXISTS is_pattern ON TABLE plugin_ui_pages TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS route_shape ON TABLE plugin_ui_pages TYPE option<string | NONE>;

-- Pages loaded before patterns existed have literal routes: shape = route.
UPDATE plugin_ui_pages SET route_shape = route WHERE route_shape = NONE;

DEFINE INDEX IF NOT EXISTS plugin_ui_pages_shape ON TABLE plugin_ui_pages COLUMNS route_shape;
DEFINE INDEX IF NOT EXISTS plugin_ui_pages_is_pattern ON TABLE plugin_ui_pages COLUMNS is_pattern;

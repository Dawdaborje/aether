-- Which set of error pages (404, 403, error cards, ...) a theme uses.
-- The web app keeps a registry of sets (`default`, `custom`, ...); a theme names one.
DEFINE FIELD IF NOT EXISTS error_pages ON TABLE plugin_themes TYPE string DEFAULT "default";

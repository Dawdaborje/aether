-- Which set of error pages (404, 403, error cards, ...) the theme uses.
DEFINE FIELD IF NOT EXISTS error_pages ON TABLE ui_themes TYPE string DEFAULT "default";

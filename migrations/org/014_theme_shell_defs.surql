-- Resolved layout / error-set definitions from the theme plugin, served with GET /api/ui/theme.
DEFINE FIELD IF NOT EXISTS layout_def ON TABLE ui_themes TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS error_pages_def ON TABLE ui_themes TYPE option<object | NONE>;
-- A plugin's text, read from its `i18n/<locale>.json` files when it is loaded:
-- { default: "en", locales: { en: { "note.title": "Title" }, fr: { ... } } }.
DEFINE FIELD IF NOT EXISTS i18n ON TABLE plugins TYPE option<object>;

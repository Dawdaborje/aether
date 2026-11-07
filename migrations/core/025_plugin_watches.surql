-- The folders a plugin wants watched (`[[watch]]` in its manifest). Installing the plugin in an
-- organization copies them into that organization's `watches`.
DEFINE FIELD IF NOT EXISTS watches ON TABLE plugins TYPE array<object> DEFAULT [];

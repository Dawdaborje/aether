-- ================================================
-- Plugin content hash and page visibility
--
-- * plugins.content_hash identifies everything a version ships (manifest,
--   pages, models, artifact) so re-loading changed content under the same
--   version is detected. artifact_hash stays the hash of the WASM file alone.
-- * Plugins may have no WASM artifact (pure UI plugins).
-- * plugin_ui_pages.is_public marks pages anonymous visitors may open;
--   models lists the models each page reads.
-- ================================================

DEFINE FIELD IF NOT EXISTS content_hash ON TABLE plugins TYPE option<string | NONE>;
-- Functions anonymous visitors may call (from the manifest's public_functions).
DEFINE FIELD IF NOT EXISTS public_functions ON TABLE plugins TYPE array<string> DEFAULT [];

DEFINE FIELD IF NOT EXISTS is_public ON TABLE plugin_ui_pages TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS models ON TABLE plugin_ui_pages TYPE array<string> DEFAULT [];

DEFINE INDEX IF NOT EXISTS plugin_ui_pages_route ON TABLE plugin_ui_pages COLUMNS route;

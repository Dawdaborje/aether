-- ================================================
-- Media guards: a stored file that a plugin decides who may read
-- (see docs/architecture/media.md, "Files a plugin guards")
--
-- A plugin that keeps files with their own permissions (a drive) puts each key under its guard with `media::protect`.
-- Reading such a key then asks the plugin's `[media_guard]` function. Keys not listed here stay readable by every
-- member of the organization, as before. Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS media_guards;
DEFINE FIELD IF NOT EXISTS key ON TABLE media_guards TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE media_guards TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE media_guards TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS media_guards_key ON TABLE media_guards COLUMNS key UNIQUE;

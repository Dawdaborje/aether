-- ================================================
-- Files that were stored on local disk because the main media storage (S3 and the like) could not be
-- reached. A background job moves each one to the main storage and then deletes this row. The file's key
-- never changes, so nothing that refers to the file has to be rewritten.
-- ================================================

DEFINE TABLE IF NOT EXISTS media_pending SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS file_key ON TABLE media_pending TYPE string;
DEFINE FIELD IF NOT EXISTS size ON TABLE media_pending TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS attempts ON TABLE media_pending TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS last_error ON TABLE media_pending TYPE option<string>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE media_pending TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE media_pending TYPE datetime DEFAULT time::now();

DEFINE INDEX IF NOT EXISTS media_pending_key ON TABLE media_pending COLUMNS file_key UNIQUE;

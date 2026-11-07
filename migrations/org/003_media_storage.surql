-- ================================================
-- Media storage
-- Storage backends configured for this org, plus
-- media objects uploaded into Aether and mapped to
-- the backend that holds them.
-- ================================================

DEFINE TABLE IF NOT EXISTS storage_backends;

DEFINE FIELD IF NOT EXISTS name ON TABLE storage_backends TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE storage_backends TYPE string;
-- "local" | "s3" | "garage" | "nextcloud" | …
DEFINE FIELD IF NOT EXISTS backend_type ON TABLE storage_backends TYPE string;
DEFINE FIELD IF NOT EXISTS is_default ON TABLE storage_backends TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS is_active ON TABLE storage_backends TYPE bool DEFAULT true;
-- Encrypted / opaque backend config (bucket, endpoint, path, …)
DEFINE FIELD IF NOT EXISTS config ON TABLE storage_backends TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE storage_backends TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE storage_backends TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS storage_backends_name ON TABLE storage_backends COLUMNS name UNIQUE;


DEFINE TABLE IF NOT EXISTS media;

DEFINE FIELD IF NOT EXISTS filename ON TABLE media TYPE string;
DEFINE FIELD IF NOT EXISTS original_filename ON TABLE media TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS mime_type ON TABLE media TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS size_bytes ON TABLE media TYPE option<int | NONE>;
DEFINE FIELD IF NOT EXISTS checksum ON TABLE media TYPE option<string | NONE>;
-- Object key / path inside the backend
DEFINE FIELD IF NOT EXISTS storage_key ON TABLE media TYPE string;
DEFINE FIELD IF NOT EXISTS storage_backend ON TABLE media TYPE record<storage_backends>;
-- Core user id string of the uploader (users:xxx)
DEFINE FIELD IF NOT EXISTS uploaded_by ON TABLE media TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS alt_text ON TABLE media TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS metadata ON TABLE media TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE media TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE media TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS media_storage_key ON TABLE media COLUMNS storage_backend, storage_key UNIQUE;
DEFINE INDEX IF NOT EXISTS media_checksum ON TABLE media COLUMNS checksum;

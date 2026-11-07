-- A plugin's files are stored as revisions: each load with changed content writes only the
-- changed files into a new time-stamped folder under plugins/<name>/ and lists every file in
-- that folder's files.json. `revision` names the folder a catalog entry's index is in.
-- Rows made before this have no revision; their files are in plugins/<name>/<version>/.
-- A rebuild of an existing version is catalogued as `<version>+<stamp>`.
DEFINE FIELD IF NOT EXISTS revision ON TABLE plugins TYPE option<string | NONE>;

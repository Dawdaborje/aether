-- ================================================
-- Watch memory (per organization)
--
-- What each watch has already reported: path, size and modification time. When a scheduler node
-- starts listening it compares the folder with this, so files that arrived, changed or went
-- away while nothing was listening are reported (catch-up), and nothing is reported twice.
-- Written only by the kernel.
-- ================================================

DEFINE FIELD IF NOT EXISTS catch_up ON TABLE watches TYPE option<bool | NONE>;

DEFINE TABLE IF NOT EXISTS watch_seen;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE watch_seen TYPE string;
DEFINE FIELD IF NOT EXISTS watch ON TABLE watch_seen TYPE string;
DEFINE FIELD IF NOT EXISTS path ON TABLE watch_seen TYPE string;
DEFINE FIELD IF NOT EXISTS size ON TABLE watch_seen TYPE int;
-- Nanoseconds since the epoch, as text (it does not fit a 64-bit number everywhere).
DEFINE FIELD IF NOT EXISTS modified_ns ON TABLE watch_seen TYPE string;
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE watch_seen TYPE datetime VALUE time::now();
DEFINE INDEX IF NOT EXISTS watch_seen_unique ON TABLE watch_seen COLUMNS plugin, watch, path UNIQUE;

-- Cross-process Mantle bust signals (CLI load/upgrade → serving process).
-- The server polls this table and invalidates in-process warm keys; rows are deleted after apply.

DEFINE TABLE IF NOT EXISTS mantle_signals;

DEFINE FIELD IF NOT EXISTS kind ON TABLE mantle_signals TYPE string;
DEFINE FIELD IF NOT EXISTS plugin ON TABLE mantle_signals TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS version ON TABLE mantle_signals TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS org ON TABLE mantle_signals TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE mantle_signals TYPE datetime DEFAULT time::now();

DEFINE INDEX IF NOT EXISTS mantle_signals_created ON TABLE mantle_signals COLUMNS created_at;

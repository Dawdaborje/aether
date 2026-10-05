-- ================================================
-- Scheduler nodes
--
-- Every process that runs jobs (the scheduler embedded in `aether --serve`, or a standalone
-- `aether --start-scheduler`) keeps one row here and refreshes `heartbeat_at` while it lives.
-- The HTTP server finds a standalone scheduler's control address from this table, and an
-- embedded scheduler stands down while a standalone one is alive.
-- ================================================

DEFINE TABLE IF NOT EXISTS scheduler_nodes;
DEFINE FIELD IF NOT EXISTS node_id ON TABLE scheduler_nodes TYPE string;
-- "embedded" | "standalone"
DEFINE FIELD IF NOT EXISTS kind ON TABLE scheduler_nodes TYPE string;
-- Where the control API listens (standalone only), such as "10.0.0.5:7895".
DEFINE FIELD IF NOT EXISTS address ON TABLE scheduler_nodes TYPE option<string | NONE>;
-- The queues this node serves.
DEFINE FIELD IF NOT EXISTS queues ON TABLE scheduler_nodes TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS version ON TABLE scheduler_nodes TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS started_at ON TABLE scheduler_nodes TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS heartbeat_at ON TABLE scheduler_nodes TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS scheduler_nodes_id ON TABLE scheduler_nodes COLUMNS node_id UNIQUE;

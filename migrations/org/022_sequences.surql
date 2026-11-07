-- ================================================
-- Naming series counters (per organization)
--
-- One row per series and period (`<model id>_<field id>[_<year>[<month>]]`) holding the last number
-- handed out. Incremented by the kernel in the same transaction as the create that uses it.
-- Written only by the kernel.
-- ================================================

DEFINE TABLE IF NOT EXISTS aether_sequence;
DEFINE FIELD IF NOT EXISTS counter ON TABLE aether_sequence TYPE int DEFAULT 0;

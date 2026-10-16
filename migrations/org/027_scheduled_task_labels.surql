-- Human-readable label and description for a scheduled task, from the plugin's `[[schedule]]`
-- entry (`title`, `description`). The key (`name`) stays the identifier.

DEFINE FIELD IF NOT EXISTS title ON TABLE scheduled_tasks TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS description ON TABLE scheduled_tasks TYPE option<string | NONE>;

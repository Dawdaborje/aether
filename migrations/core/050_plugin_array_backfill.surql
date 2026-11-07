-- Catalog rows written before a list field was added have no value for it: a DEFAULT only fills new
-- records. Any later UPDATE of such a row (the catalog load stores `source_hash` on every reload) then
-- fails with "Couldn't coerce value for field `ai_tools` ... Expected `array` but found `NONE`".
-- Give every such row an empty list.

UPDATE plugins SET ai_tools = [] WHERE ai_tools = NONE;
UPDATE plugins SET roles = [] WHERE roles = NONE;
UPDATE plugins SET event_listeners = [] WHERE event_listeners = NONE;
UPDATE plugins SET watches = [] WHERE watches = NONE;
UPDATE plugins SET commands = [] WHERE commands = NONE;
UPDATE plugins SET schedules = [] WHERE schedules = NONE;
UPDATE plugins SET public_functions = [] WHERE public_functions = NONE;

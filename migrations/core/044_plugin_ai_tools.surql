-- The functions a plugin offers to the AI assistant (`[[ai_tool]]` in its manifest):
-- { name, description, function, schema, writes }.
DEFINE FIELD IF NOT EXISTS ai_tools ON TABLE plugins TYPE array<object> DEFAULT [];

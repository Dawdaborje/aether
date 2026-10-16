-- A plugin's rendered documentation (`[docs]` in its manifest): { plugin, version, label, visibility, pages }.
-- Written when the plugin is loaded; read by the documentation website and the MCP server.
DEFINE FIELD IF NOT EXISTS docs ON TABLE plugins TYPE option<object>;

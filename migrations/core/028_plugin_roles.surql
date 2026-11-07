-- The roles a plugin offers (`[[roles]]` in its manifest): { name, label, description }.
-- Installing the plugin in an organization creates them as `<plugin>.<name>` in that
-- organization's `roles`, for an administrator to give to people.
DEFINE FIELD IF NOT EXISTS roles ON TABLE plugins TYPE array<object> DEFAULT [];

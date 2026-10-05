-- The commands a plugin offers to the command line (`[[command]]` in its manifest):
-- { name, function, help, args: [{ name, help, type, required, default }] }.
DEFINE FIELD IF NOT EXISTS commands ON TABLE plugins TYPE array<object> DEFAULT [];

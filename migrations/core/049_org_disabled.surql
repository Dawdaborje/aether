-- A disabled organization: members cannot use it and background work skips it until a developer
-- enables it again. Nothing in it is removed.

DEFINE FIELD IF NOT EXISTS disabled_at ON TABLE organizations TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS disabled_by ON TABLE organizations TYPE option<string>;

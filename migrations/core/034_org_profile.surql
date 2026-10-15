-- Organization profile: description, parent org, and staged logo filename.

DEFINE FIELD IF NOT EXISTS description ON TABLE organizations TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS parent_db_name ON TABLE organizations TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS logo_file ON TABLE organizations TYPE option<string | NONE>;

DEFINE INDEX IF NOT EXISTS organizations_parent ON TABLE organizations COLUMNS parent_db_name;

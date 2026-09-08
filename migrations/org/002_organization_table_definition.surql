
DEFINE TABLE IF NOT EXISTS organizations;

DEFINE FIELD IF NOT EXISTS name ON TABLE organizations TYPE string;
DEFINE FIELD IF NOT EXISTS description  ON TABLE organizations TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS logo ON TABLE organizations TYPE option<string> DEFAULT "LOGO";
DEFINE FIELD IF NOT EXISTS logo_type ON TABLE organizations TYPE option<"svg" | "image" | "char">;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE organizations TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE organizations TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS organizations_name ON TABLE organizations COLUMNS name UNIQUE;
-- ==============================
-- Create companies table
-- ==============================

DEFINE TABLE IF NOT EXISTS companies;

DEFINE FIELD IF NOT EXISTS email ON TABLE users TYPE option<string | None>
    ASSERT string::is_email($value);

DEFINE FIELD IF NOT EXISTS code ON TABLE companies TYPE string
    ASSERT string::len($value) <= 50;

DEFINE FIELD IF NOT EXISTS name ON TABLE companies TYPE string
    ASSERT string::len($value) <= 150;

DEFINE FIELD IF NOT EXISTS description ON TABLE companies TYPE option<string | None>;

DEFINE FIELD IF NOT EXISTS date_created ON TABLE companies TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE companies TYPE datetime VALUE time::now();


DEFINE INDEX IF NOT EXISTS idx_companies_email ON TABLE companies COLUMNS email;
DEFINE INDEX IF NOT EXISTS idx_companies_code ON TABLE companies COLUMNS code;
DEFINE INDEX IF NOT EXISTS idx_companies_name ON TABLE companies COLUMNS name;
DEFINE INDEX IF NOT EXISTS idx_companies_description ON TABLE companies COLUMNS description;
DEFINE INDEX IF NOT EXISTS idx_companies_date_created ON TABLE companies COLUMNS date_created;
DEFINE INDEX IF NOT EXISTS idx_companies_date_updated ON TABLE companies COLUMNS date_updated;
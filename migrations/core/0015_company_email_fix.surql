-- Repair migration for installations that already applied 0013_company.
DEFINE FIELD IF NOT EXISTS email ON TABLE companies TYPE option<string | NONE>
    ASSERT $value = NONE OR string::is_email($value);

DEFINE INDEX IF NOT EXISTS idx_companies_email ON TABLE companies COLUMNS email UNIQUE;

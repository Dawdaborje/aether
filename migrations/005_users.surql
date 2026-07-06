-- ================================================
-- Users table
-- ================================================

DEFINE TABLE IF NOT EXISTS users;

DEFINE FIELD IF NOT EXISTS email ON TABLE users TYPE option<string | None>
    ASSERT string::is_email($value);

DEFINE FIELD IF NOT EXISTS hashed_password ON TABLE users TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_super_user ON TABLE users TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE users TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE users TYPE datetime VALUE time::now();


-- =========================================================================================
-- Org users, this tells where the user belongs and does the user have access the orgs below
-- ==========================================================================================
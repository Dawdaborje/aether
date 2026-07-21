-- ================================================
-- Invitations
-- Invite a person (by email) into this org before
-- they become an org_users row.
-- ================================================

DEFINE TABLE IF NOT EXISTS invitations;

DEFINE FIELD IF NOT EXISTS email ON TABLE invitations TYPE string
    ASSERT string::is_email($value);
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE invitations TYPE string;
-- Optional role / group to assign on accept
DEFINE FIELD IF NOT EXISTS role ON TABLE invitations TYPE option<record<roles> | NONE>;
DEFINE FIELD IF NOT EXISTS group ON TABLE invitations TYPE option<record<groups> | NONE>;
-- Core user id of who sent the invite
DEFINE FIELD IF NOT EXISTS invited_by ON TABLE invitations TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS accepted_at ON TABLE invitations TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE invitations TYPE option<datetime | NONE>;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE invitations TYPE datetime;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE invitations TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE invitations TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS invitations_token_hash ON TABLE invitations COLUMNS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS invitations_email ON TABLE invitations COLUMNS email;

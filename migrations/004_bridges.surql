-- ================================================
-- Bridges (platform catalog)
-- First-party integrations compiled into the kernel
-- (Stripe, Paystack, Resend, Mayan, …).
-- Per-org credentials / enablement live in
-- org_migrations (bridge_configs).
-- ================================================

DEFINE TABLE IF NOT EXISTS bridges;

DEFINE FIELD IF NOT EXISTS name ON TABLE bridges TYPE string;
DEFINE FIELD IF NOT EXISTS label ON TABLE bridges TYPE string;
DEFINE FIELD IF NOT EXISTS description ON TABLE bridges TYPE option<string | NONE>;
-- e.g. "payments" | "communications" | "auth" | "storage" | "documents"
DEFINE FIELD IF NOT EXISTS category ON TABLE bridges TYPE string;
-- Cargo feature / crate id, e.g. "stripe", "resend"
DEFINE FIELD IF NOT EXISTS feature_key ON TABLE bridges TYPE string;
DEFINE FIELD IF NOT EXISTS version ON TABLE bridges TYPE option<string | NONE>;
DEFINE FIELD IF NOT EXISTS is_builtin ON TABLE bridges TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS enabled_globally ON TABLE bridges TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS config_schema ON TABLE bridges TYPE option<object | NONE>;
DEFINE FIELD IF NOT EXISTS date_created ON TABLE bridges TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS date_updated ON TABLE bridges TYPE datetime VALUE time::now();

DEFINE INDEX IF NOT EXISTS bridges_name ON TABLE bridges COLUMNS name UNIQUE;
DEFINE INDEX IF NOT EXISTS bridges_feature_key ON TABLE bridges COLUMNS feature_key UNIQUE;

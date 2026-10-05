-- Mirrors the global flag: an organization's own value for a secret setting is stored encrypted.
DEFINE FIELD IF NOT EXISTS is_secret ON TABLE settings_items TYPE bool DEFAULT false;

-- Secret settings (API keys, passwords) are stored encrypted and never shown again once saved.
DEFINE FIELD IF NOT EXISTS is_secret ON TABLE gl_settings_items TYPE bool DEFAULT false;

-- Tasks a plugin declares with `[[schedule]]` in its manifest. Installing the plugin in an
-- organization copies them into that organization's `scheduled_tasks`.
DEFINE FIELD IF NOT EXISTS schedules ON TABLE plugins TYPE array<object> DEFAULT [];

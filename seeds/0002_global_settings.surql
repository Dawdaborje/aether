-- =====================================================
-- SEED DATA: SETTINGS GROUPS
-- =====================================================

LET $group_general = type::record("gl_settings_groups", rand::uuid::v7());
LET $group_security = type::record("gl_settings_groups", rand::uuid::v7());
LET $group_notifications = type::record("gl_settings_groups", rand::uuid::v7());

INSERT INTO gl_settings_groups [
    {
        id: $group_general,
        label: "General Settings",
        color: "blue",
        icon: "cog",
        icon_type: "lucide"
    },
    {
        id: $group_security,
        label: "Security & Privacy",
        color: "red",
        icon: "shield",
        icon_type: "lucide"
    },
    {
        id: $group_notifications,
        label: "Notifications",
        color: "green",
        icon: "bell",
        icon_type: "lucide"
    }
];

-- =====================================================
-- SEED DATA: SETTINGS ITEMS
-- =====================================================

LET $item_app_name = type::record("gl_settings_items", rand::uuid::v7());
LET $item_mfa = type::record("gl_settings_items", rand::uuid::v7());
LET $item_alerts = type::record("gl_settings_items", rand::uuid::v7());
LET $item_auth_primary = type::record("gl_settings_items", rand::uuid::v7());
LET $item_auth_methods = type::record("gl_settings_items", rand::uuid::v7());
LET $item_auth_reg = type::record("gl_settings_items", rand::uuid::v7());

INSERT INTO gl_settings_items [
    {
        id: $item_app_name,
        label: "Application Name",
        s_key: "app_name",
        s_value: "My Awesome App",
        description: "The public-facing name of the application.",
        long_description: "This name will appear in emails, browser tabs, and the main dashboard header."
    },
    {
        id: $item_mfa,
        label: "Enforce Multi-Factor Authentication",
        s_key: "mfa_required",
        s_value: true,
        description: "Require all staff accounts to use MFA.",
        long_description: "When enabled, users without MFA configured will be forced to set it up on their next login."
    },
    {
        id: $item_alerts,
        label: "Email Alert Digest",
        s_key: "email_digest_frequency",
        s_value: "daily",
        description: "How often system alert summaries are sent.",
        long_description: "Choose between 'realtime', 'daily', or 'weekly' summaries for system-level event logging."
    },
    {
        id: $item_auth_primary,
        label: "Primary Auth Method",
        s_key: "auth.primary_method",
        s_value: "local",
        description: "Default login method shown first.",
        long_description: "Use `local` for username/password, or a bridge name such as `google`, `keycloak`, etc."
    },
    {
        id: $item_auth_methods,
        label: "Enabled Auth Methods",
        s_key: "auth.enabled_methods",
        s_value: ["local"],
        description: "Auth methods exposed on the login page and /api/auth/methods.",
        long_description: "Array of method ids. Always include `local` unless the instance is IdP-only."
    },
    {
        id: $item_auth_reg,
        label: "Allow Local Registration",
        s_key: "auth.allow_registration",
        s_value: false,
        description: "Permit self-service local account creation.",
        long_description: "When false, only administrators (or IdP provisioning) can create users."
    }
];

-- =====================================================
-- GROUP ↔ ITEM RELATION TABLE
-- =====================================================

INSERT INTO gl_settings_group_items [
    {
        id: type::record("gl_settings_group_items", rand::uuid::v7()),
        group: $group_general,
        item: $item_app_name
    },
    {
        id: type::record("gl_settings_group_items", rand::uuid::v7()),
        group: $group_security,
        item: $item_mfa
    },
    {
        id: type::record("gl_settings_group_items", rand::uuid::v7()),
        group: $group_security,
        item: $item_auth_primary
    },
    {
        id: type::record("gl_settings_group_items", rand::uuid::v7()),
        group: $group_security,
        item: $item_auth_methods
    },
    {
        id: type::record("gl_settings_group_items", rand::uuid::v7()),
        group: $group_security,
        item: $item_auth_reg
    },
    {
        id: type::record("gl_settings_group_items", rand::uuid::v7()),
        group: $group_notifications,
        item: $item_alerts
    }
];

-- =====================================================
-- Default Google auth provider row (disabled until configured)
-- =====================================================

INSERT INTO auth_providers {
    name: "google",
    label: "Google",
    provider_type: "oauth2",
    issuer_url: "https://accounts.google.com",
    scopes: ["openid", "email", "profile"],
    enabled: false,
    config: {
        authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
        token_url: "https://oauth2.googleapis.com/token",
        userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo"
    }
};

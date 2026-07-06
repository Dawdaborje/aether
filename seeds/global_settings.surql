-- =====================================================
-- SEED DATA: SETTINGS GROUPS
-- =====================================================

-- Generate and store group IDs using type::record()
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

-- Generate and store item IDs using type::record()
LET $item_app_name = type::record("gl_settings_items", rand::uuid::v7());
LET $item_mfa = type::record("gl_settings_items", rand::uuid::v7());
LET $item_alerts = type::record("gl_settings_items", rand::uuid::v7());

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
    }
];

-- =====================================================
-- SEED DATA: GROUP ↔ ITEM RELATION TABLE
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
        group: $group_notifications,
        item: $item_alerts
    }
];
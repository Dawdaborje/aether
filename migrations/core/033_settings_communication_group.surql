-- Merge Email, SMS and provider bridge groups into one Communication category.
-- Setting items stay put; only group membership and empty groups change.

UPSERT gl_settings_groups:communication CONTENT {
	label: "Communication",
	color: "blue",
	icon: "messages-square",
	icon_type: "lucide",
	date_created: time::now(),
	date_updated: time::now()
};

UPDATE gl_settings_group_items SET
	group = gl_settings_groups:communication,
	date_updated = time::now()
WHERE group INSIDE [
	gl_settings_groups:comm_email,
	gl_settings_groups:comm_sms,
	gl_settings_groups:bridge_smtp,
	gl_settings_groups:bridge_resend,
	gl_settings_groups:bridge_twilio,
	gl_settings_groups:bridge_termii,
	gl_settings_groups:bridge_africas_talking
];

DELETE gl_settings_groups:comm_email;
DELETE gl_settings_groups:comm_sms;
DELETE gl_settings_groups:bridge_smtp;
DELETE gl_settings_groups:bridge_resend;
DELETE gl_settings_groups:bridge_twilio;
DELETE gl_settings_groups:bridge_termii;
DELETE gl_settings_groups:bridge_africas_talking;

-- Prefer clearer icons on remaining groups.
UPDATE gl_settings_groups:general SET icon = "settings", icon_type = "lucide", date_updated = time::now();
UPDATE gl_settings_groups:notifications SET icon = "bell", icon_type = "lucide", date_updated = time::now();
UPDATE gl_settings_groups:security SET icon = "shield", icon_type = "lucide", date_updated = time::now();
UPDATE gl_settings_groups:scheduler SET icon = "clock", icon_type = "lucide", date_updated = time::now();
UPDATE gl_settings_groups:chatter SET icon = "message-square", icon_type = "lucide", date_updated = time::now();
UPDATE gl_settings_groups:bridge_paystack SET icon = "credit-card", icon_type = "lucide", date_updated = time::now();
UPDATE gl_settings_groups:bridge_open_street_map SET icon = "map-pin", icon_type = "lucide", date_updated = time::now();

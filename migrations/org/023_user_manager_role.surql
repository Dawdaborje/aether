-- Kernel role: manage organization members and roles (owner and org_admin do not need it).
UPSERT roles SET
	name = "user_manager",
	label = "User manager",
	description = "Lists and manages members and roles in this organization",
	is_system = true
WHERE name = "user_manager";

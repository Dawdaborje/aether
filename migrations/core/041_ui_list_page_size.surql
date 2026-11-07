-- Default list page size (global). Organizations may override via settings.

UPSERT gl_settings_groups:ui CONTENT {
	label: "User Interface",
	color: "slate",
	icon: "layout",
	icon_type: "lucide",
	date_created: time::now(),
	date_updated: time::now()
};

UPSERT gl_settings_items:ui_list_page_size CONTENT {
	label: "List rows per page",
	s_key: "ui.list.page_size",
	s_value: 10,
	is_secret: false,
	description: "Default number of rows shown in data tables and plugin lists.",
	long_description: "Used when a list does not set its own page size. Choose 10, 25, 50, or 100. Organizations can override the global default.",
	date_created: time::now(),
	date_updated: time::now()
};

UPSERT gl_settings_group_items:ui_ui_list_page_size CONTENT {
	group: gl_settings_groups:ui,
	item: gl_settings_items:ui_list_page_size,
	date_created: time::now(),
	date_updated: time::now()
};

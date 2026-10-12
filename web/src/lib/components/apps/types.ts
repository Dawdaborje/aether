/** One top-bar menu entry for an installed app (`GET /api/ui/apps` → `menus`). */
export interface AppMenuItem {
	label: string;
	href: string;
	icon?: string | null;
}

/** One tile of the Apps launcher, as `GET /api/ui/apps` returns it. */
export interface AppTileData {
	plugin: string;
	/** The plugin's workspace; groups apps into categories. */
	category?: string | null;
	label: string;
	icon: string | null;
	/** `/api/ui/plugins/{name}/logo` when the revision ships a logo file. */
	logo_url?: string | null;
	route: string;
	description: string | null;
	/** Main pages of this plugin for the navbar when theme nav does not match. */
	menus?: AppMenuItem[];
}

/** One top-bar / sidebar menu entry for an installed app (`GET /api/ui/apps` → `menus`). */
export interface AppMenuItem {
	label: string;
	href: string;
	icon?: string | null;
	/** Collapsible sidebar section (page `menu_group`). */
	group?: string | null;
}

/** Workspace group on the Apps launcher (`GET /api/ui/apps` → `categories`). */
export interface AppCategory {
	/** Workspace name (`base`, `crm`); matches `AppTileData.category`. */
	id: string;
	label: string;
	/** Lucide icon id when the workspace did not ship a logo file. */
	icon?: string | null;
	/** `/api/ui/workspaces/{id}/logo` (or a member app logo before reload). */
	logo_url?: string | null;
	/** `folder` (default) or `flat` — flat lists each member on the home grid. */
	launcher?: string | null;
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
	/** Named Apps-home folder (`[app] folder`); shared within a workspace. */
	folder?: string | null;
	/** Main pages of this plugin for the navbar when theme nav does not match. */
	menus?: AppMenuItem[];
}

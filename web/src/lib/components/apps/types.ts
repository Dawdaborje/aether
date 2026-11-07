/** One top-bar / sidebar menu entry for an installed app (`GET /api/ui/apps` → `menus`). */
export interface AppMenuItem {
	label: string;
	href: string;
	icon?: string | null;
	/** Collapsible sidebar section (page `menu_group`). */
	group?: string | null;
	/** Lucide icon for that section (page `menu_group_icon`). */
	group_icon?: string | null;
	/** `plugin.function` whose answer is shown as a count (page `menu_badge`). */
	badge?: string | null;
	/** A line under the entry (page `menu_description`), shown by the `mega` nav variant. */
	description?: string | null;
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
	/** The workspace description, shown under its folder tile. */
	description?: string | null;
}

/** An `[app] nav = true` plugin: a link beside Users / Settings, not a launcher tile. */
export interface AppNavLink {
	plugin: string;
	label: string;
	icon: string | null;
	href: string;
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
	/** A live number for the tile (`[app.tile]`); `function` is `plugin.function`. */
	tile?: { function: string; label: string | null; unit: string | null } | null;
	/** Why the plugin could not be loaded; shown as a badge on the tile (desk catalog only). */
	problem?: string | null;
}

/** An `[[app.palette]]` entry the person may use (`GET /api/ui/apps` → `palette`). */
export interface AppPaletteEntry {
	plugin: string;
	/** The app's label, shown beside the entry. */
	app: string;
	label: string;
	/** What the entry opens; `null` for a function or a record search. */
	href: string | null;
	/** `plugin.function` the entry runs, or `null`. */
	function: string | null;
	/** `plugin.function` that finds records for what is typed, or `null`. */
	search: string | null;
	icon: string | null;
	keywords: string | null;
	/** Normalised: `mod`, `alt`, `shift` joined by `+`, then a letter or digit. */
	shortcut: string | null;
	/** Listed under the shell's "New" button (`create` block). */
	create?: boolean;
}

/** A setup page a plugin asks people to complete (`GET /api/ui/apps` → `setup`). */
export interface AppSetupStep {
	plugin: string;
	title: string;
	description?: string | null;
	href: string;
	/** Open the page's form in a dialog on the Apps page instead of going to the page. */
	modal?: boolean;
}

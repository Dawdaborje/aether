/** One tile of the Apps launcher, as `GET /api/ui/apps` returns it. */
export interface AppTileData {
	plugin: string;
	/** The plugin's workspace; groups apps into categories. */
	category?: string | null;
	label: string;
	icon: string | null;
	route: string;
	description: string | null;
}

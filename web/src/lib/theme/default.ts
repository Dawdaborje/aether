import manifest from './default-theme.json';
import type { ThemeConfig, ThemeDensity } from './types';

/*
 * The built-in default theme: what the app looks like when an organization has no theme
 * plugin installed: IBM Carbon (White / Gray 100) set in IBM Plex. Flat layers, square
 * corners and one blue (`#0f62fe`) for actions and focus. The earlier neutral look ships
 * as the `theme_neutral` plugin.
 *
 * `default-theme.json` is a theme manifest in the same format a theme plugin ships, and
 * the one source of these values: the server's fallback (`facets/web/src/api/theme.rs`)
 * embeds it, and `routes/layout.css` repeats it for the first paint (a test keeps them
 * in step).
 */
export const defaultTheme: ThemeConfig = {
	name: manifest.name,
	label: manifest.label,
	colorMode: 'system',
	source: 'fallback',
	layout: manifest.layout,
	errorPages: 'default',
	nav: null,
	layoutDef: null,
	errorPagesDef: null,
	shell: null,
	tokens: {
		light: manifest.light,
		dark: manifest.dark,
		radius: manifest.radius,
		fontSans: manifest.font_sans,
		fontMono: manifest.font_mono,
		density: manifest.density as ThemeDensity
	}
};

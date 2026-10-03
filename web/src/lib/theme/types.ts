export type ThemeColorMode = 'light' | 'dark' | 'system';

export type ThemeTokenMap = Record<string, string>;

export interface ThemeTokens {
	light: ThemeTokenMap;
	dark: ThemeTokenMap;
	radius: string;
	fontSans: string;
}

/** One entry of a theme's navigation: a link, a group of links, or both. */
export interface NavItem {
	label: string;
	/** An app path (`/apps`) or an http(s) URL. */
	href?: string;
	/** Icon name (reserved for layouts that draw icons). */
	icon?: string;
	children?: NavItem[];
}

/** The navigation a theme gives its layout. */
export interface ThemeNav {
	header?: string;
	items: NavItem[];
}

export interface ThemeConfig {
	name: string;
	label: string;
	colorMode: ThemeColorMode;
	/** `organization`: the organization's active theme; `fallback`: the built-in one. */
	source: 'organization' | 'fallback';
	tokens: ThemeTokens;
	/** Which layout component renders the app; see `components/layout/registry.ts`. */
	layout: string;
	/** Which set of error pages the app shows; see `components/errors/registry.ts`. */
	errorPages: string;
	/** Navigation from the theme, or null to use the layout's own. */
	nav: ThemeNav | null;
}

export interface ThemeApiResponse {
	name: string;
	label: string;
	color_mode: string;
	source: string;
	tokens: {
		light: ThemeTokenMap;
		dark: ThemeTokenMap;
		radius: string;
		font_sans: string;
	};
	layout?: string;
	error_pages?: string;
	nav?: ThemeNav | null;
}

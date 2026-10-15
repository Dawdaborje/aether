export type ThemeColorMode = 'light' | 'dark' | 'system';

export type ThemeTokenMap = Record<string, string>;

export type ThemeDensity = 'compact' | 'comfortable' | 'spacious';

export interface ThemeTokens {
	light: ThemeTokenMap;
	dark: ThemeTokenMap;
	radius: string;
	fontSans: string;
	fontMono?: string;
	fontHeading?: string;
	density?: ThemeDensity;
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

/** Where a block shows: signed-in users, developers, user managers or org admins. */
export type BlockWhen = 'user' | 'developer' | 'manager' | 'admin';

/** One block of a composed layout region (validated by the server's `layout_blocks.rs`). */
export interface LayoutBlock {
	type: string;
	id?: string;
	props?: Record<string, string | boolean>;
	when?: BlockWhen;
	/** Any of these roles shows the block. */
	roles?: string[];
	/** Hidden on screens narrower than this breakpoint. */
	hide_below?: 'sm' | 'md' | 'lg' | 'xl';
}

export type RegionTone = 'default' | 'muted' | 'primary' | 'inverted';

export interface LayoutRegion {
	blocks: LayoutBlock[];
	/** A CSS length (sidebar / aside). */
	width?: string;
	position?: 'left' | 'right';
	tone?: RegionTone;
	sticky?: boolean;
}

export type RegionName = 'banner' | 'topbar' | 'sidebar' | 'aside' | 'footer';

/** Plugin-shipped layout definition (`layouts/<name>.json`). */
export interface LayoutDef {
	name: string;
	label?: string;
	/** `top` | `sidebar` | `custom` | `bare`, or `composed` when `regions` is given. */
	chrome: string;
	regions?: Partial<Record<RegionName, LayoutRegion>>;
}

/** Theme settings beyond colours. */
export interface ThemeShell {
	/** Where `/`, the brand and "home" links lead; the app launcher when absent. */
	home?: { route: string };
	branding?: { product_name?: string; logo?: string; favicon?: string };
	/** The theme's stylesheet, already checked and rewritten by the server. */
	css?: string;
}

/** Optional copy override for one error surface. */
export interface ErrorCopyOverride {
	title?: string;
	message?: string;
}

/** Plugin-shipped error-page set (`errors/<name>.json`). */
export interface ErrorSetDef {
	name: string;
	label?: string;
	/** `default` | `custom` — first-party look to base on. */
	variant: string;
	not_found?: ErrorCopyOverride;
	forbidden?: ErrorCopyOverride;
	too_many_requests?: ErrorCopyOverride;
	server_error?: ErrorCopyOverride;
	card?: ErrorCopyOverride;
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
	/** Present when `layout` is a plugin-defined shell. */
	layoutDef: LayoutDef | null;
	/** Present when `error_pages` is a plugin-defined set. */
	errorPagesDef: ErrorSetDef | null;
	shell: ThemeShell | null;
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
		font_mono?: string;
		font_heading?: string;
		density?: string;
	};
	layout?: string;
	error_pages?: string;
	nav?: ThemeNav | null;
	layout_def?: LayoutDef | null;
	error_pages_def?: ErrorSetDef | null;
	shell?: ThemeShell | null;
}

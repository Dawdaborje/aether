export type ThemeColorMode = 'light' | 'dark' | 'system';

export type ThemeTokenMap = Record<string, string>;

export type ThemeDensity = 'compact' | 'comfortable' | 'spacious';

/** Steps of the corner-radius scale (`rounded-sm`, `rounded-lg`, …). */
export const SHAPE_STEPS = ['xs', 'sm', 'md', 'lg', 'xl', '2xl', '3xl', '4xl'] as const;
/** What the app rounds by role (`rounded-button`, `rounded-card`, …). */
export const SHAPE_ROLES = [
	'button',
	'field',
	'card',
	'chip',
	'menu',
	'dialog',
	'tooltip',
	'tile',
	'avatar'
] as const;

export type ShapeStep = (typeof SHAPE_STEPS)[number];
export type ShapeRole = (typeof SHAPE_ROLES)[number];

/**
 * Corner radii beyond the single `radius`: lengths for steps of the scale, and for each role
 * a step, `none`, `full` or a length, or up to four of those for the corners (top-left,
 * top-right, bottom-right, bottom-left). Checked by the server (`validate_shape`).
 */
export interface ThemeShape {
	scale?: Partial<Record<ShapeStep, string>>;
	roles?: Partial<Record<ShapeRole, string>>;
}

/** Steps of the font-size scale (`text-sm`, `text-lg`, …). */
export const TYPE_STEPS = ['xs', 'sm', 'base', 'lg', 'xl', '2xl', '3xl', '4xl', '5xl'] as const;
/** What the app sets text in, by role (`type-title`, `type-label`, …). */
export const TYPE_ROLES = [
	'display',
	'headline',
	'title',
	'heading',
	'subheading',
	'body',
	'label',
	'caption',
	'metric',
	'button',
	'column'
] as const;

export type TypeStep = (typeof TYPE_STEPS)[number];
export type TypeRole = (typeof TYPE_ROLES)[number];

/** How one role sets its text. Every property is optional; `layout.css` has the defaults. */
export interface ThemeTypeRole {
	/** A step of the scale or a length. */
	size?: string;
	/** A unitless number or a length. */
	line_height?: number | string;
	/** 100 to 900. */
	weight?: number | string;
	/** `normal` or a length in em, px or rem. */
	tracking?: string;
	family?: 'sans' | 'heading' | 'mono';
	case?: 'none' | 'upper' | 'lower' | 'capitalize';
}

/**
 * The type scale and roles: lengths for steps of the scale, and how each role sets its text.
 * Checked by the server (`validate_type`).
 */
export const ELEVATION_STEPS = ['2xs', 'xs', 'sm', 'md', 'lg', 'xl', '2xl'] as const;
/** Shadow lists by step (`shadow-sm`, …); `none` removes the shadow. */
export type ThemeElevation = Partial<Record<(typeof ELEVATION_STEPS)[number], string>>;
/** Motion tokens: the two durations and two easings the components use. */
export interface ThemeMotion {
	'duration-short'?: string;
	'duration-medium'?: string;
	'easing-standard'?: string;
	'easing-emphasized'?: string;
}
/** Focus indicator: `ring` outlines the element, `fill` paints it with `focus-fill`/`focus-text`. */
export interface ThemeFocus {
	width?: string;
	offset?: string;
	style?: 'ring' | 'fill';
}
/** How inputs are drawn: Carbon's bottom line, a full outline, or a filled box. */
export interface ThemeField {
	style?: 'underline' | 'outline' | 'filled';
}
/** How lists and tables are drawn. */
export interface ThemeTable {
	header?: 'filled' | 'plain' | 'bold';
	rows?: 'lines' | 'zebra' | 'plain';
	density?: 'compact' | 'default' | 'relaxed';
}
/** How `/apps` lays out the installed apps. */
export interface ThemeLauncher {
	variant?: 'grid' | 'list' | 'cards';
	width?: 'narrow' | 'default' | 'wide' | 'full';
}
/** Colours that replace the theme's while one app is open, keyed by app name. */
export type ThemeApps = Record<
	string,
	{ primary?: string; header?: string; header_foreground?: string }
>;

export interface ThemeType {
	scale?: Partial<Record<TypeStep, string>>;
	roles?: Partial<Record<TypeRole, ThemeTypeRole>>;
}

export interface ThemeTokens {
	light: ThemeTokenMap;
	dark: ThemeTokenMap;
	radius: string;
	fontSans: string;
	fontMono?: string;
	fontHeading?: string;
	density?: ThemeDensity;
	shape?: ThemeShape;
	type?: ThemeType;
	elevation?: ThemeElevation;
	motion?: ThemeMotion;
	focus?: ThemeFocus;
	apps?: ThemeApps;
	launcher?: ThemeLauncher;
	field?: ThemeField;
	table?: ThemeTable;
}

/** One entry of a theme's navigation: a link, a group of links, or both. */
export interface NavItem {
	label: string;
	/** An app path (`/apps`) or an http(s) URL. */
	href?: string;
	/** Icon name; every nav variant draws it. */
	icon?: string;
	/** A line under the label where the layout has room for it (the `mega` nav variant). */
	description?: string;
	/** A fixed label (`New`) or a function whose answer is a count for the person looking. */
	badge?: NavBadge;
	children?: NavItem[];
}

/** A nav item's badge: fixed text, or `{ function: "plugin.function" }` answering a count. */
export type NavBadge = string | { function: string };

/** The navigation a theme gives its layout. */
export interface ThemeNav {
	header?: string;
	items: NavItem[];
	/** The links a `nav` block with `source: "footer"` draws, in columns. */
	footer?: NavItem[];
}

/** Where a block shows: signed-in users, developers, user managers or org admins. */
export type BlockWhen = 'user' | 'developer' | 'manager' | 'admin';

/** One block of a composed layout region (validated by the server's `layout_blocks.rs`). */
export interface LayoutBlock {
	type: string;
	id?: string;
	props?: Record<string, string | boolean>;
	/** Links and groups written inside a `nav` or `app-menu` block; they replace what it would read. */
	items?: NavItem[];
	when?: BlockWhen;
	/** Any of these roles shows the block. */
	roles?: string[];
	/** Hidden on screens narrower than this breakpoint. */
	hide_below?: 'sm' | 'md' | 'lg' | 'xl';
	/** Only shown on screens narrower than this breakpoint. */
	show_below?: 'sm' | 'md' | 'lg' | 'xl';
}

export type RegionTone = 'default' | 'muted' | 'primary' | 'inverted';

export interface LayoutRegion {
	blocks: LayoutBlock[];
	/** A CSS length (sidebar / aside). */
	width?: string;
	position?: 'left' | 'right';
	tone?: RegionTone;
	sticky?: boolean;
	/** Sidebar: `rail` keeps a narrow strip of icons; `collapsible` lets the person fold it. */
	mode?: 'full' | 'rail' | 'collapsible';
	/** Width of the rail or folded sidebar (a CSS length). */
	collapsed_width?: string;
	/** Below `md` the sidebar opens over the page from the `sidebar-toggle` block. */
	overlay?: boolean;
	/** Only shown on screens narrower than this breakpoint. */
	show_below?: 'sm' | 'md' | 'lg' | 'xl';
}

export type RegionName =
	| 'banner'
	| 'topbar'
	| 'subbar'
	| 'sidebar'
	| 'aside'
	| 'bottombar'
	| 'footer';

/** Plugin-shipped layout definition (`layouts/<name>.json`). */
export interface LayoutDef {
	name: string;
	label?: string;
	/** `top` | `sidebar` | `custom` | `bare`, or `composed` when `regions` is given. */
	chrome: string;
	regions?: Partial<Record<RegionName, LayoutRegion>>;
}

/** Theme settings beyond colours. */
/** One block of a sign-in screen (`screens/<name>.xml`). */
export interface ScreenBlock {
	type:
		| 'brand'
		| 'heading'
		| 'text'
		| 'image'
		| 'link'
		| 'divider'
		| 'spacer'
		| 'login-form'
		| 'oauth-buttons'
		| 'registration-notice'
		| 'forgot-password-notice'
		| 'reset-password-form';
	props?: Record<string, string | number | boolean>;
}
/** A sign-in screen a theme ships: an optional side panel and the main column. */
export interface ScreenDef {
	name: 'login' | 'register' | 'forgot-password' | 'reset-password';
	layout: 'split' | 'center' | 'stack';
	aside?: { tone?: 'default' | 'muted' | 'primary'; side?: 'left' | 'right'; blocks: ScreenBlock[] };
	main: { blocks: ScreenBlock[] };
}

export interface ThemeShell {
	/** Where `/`, the brand and "home" links lead; the app launcher when absent. */
	home?: { route: string };
	branding?: { product_name?: string; logo?: string; favicon?: string };
	/** The theme's stylesheet, already checked and rewritten by the server. */
	css?: string;
	/** The theme's icons by name, from its SVG sprite, checked and rebuilt by the server. */
	icons?: Record<string, ThemeIcon>;
	/** Sign-in screens the theme replaces, by name. */
	screens?: Partial<Record<ScreenDef['name'], ScreenDef>>;
}

/** One icon of a theme's sprite: a `<symbol>` the server has checked and rewritten. */
export interface ThemeIcon {
	view_box: string;
	/** Presentation attributes of the symbol (`fill`, `stroke`, …). */
	attrs: Record<string, string>;
	/** The symbol's inner markup: plain shapes only. */
	body: string;
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
		shape?: ThemeShape;
		type?: ThemeType;
		elevation?: ThemeElevation;
		motion?: ThemeMotion;
		focus?: ThemeFocus;
		apps?: ThemeApps;
		launcher?: ThemeLauncher;
		field?: ThemeField;
		table?: ThemeTable;
	};
	layout?: string;
	error_pages?: string;
	nav?: ThemeNav | null;
	layout_def?: LayoutDef | null;
	error_pages_def?: ErrorSetDef | null;
	shell?: ThemeShell | null;
}

import type { ThemeConfig, ThemeTokenMap } from './types';

/*
 * The built-in default theme: what the app looks like when an organization has no
 * theme plugin installed. Enterprise, and deliberately simple: a warm cream
 * canvas, ink text, one amber-yellow accent, a deep umber top bar, hairline borders and
 * a modest radius. `routes/layout.css` carries the same values so the first paint
 * (before any script runs) already matches.
 */
const light: ThemeTokenMap = {
	background: 'oklch(0.985 0.012 92)',
	foreground: 'oklch(0.24 0.03 70)',
	card: 'oklch(1 0 0)',
	'card-foreground': 'oklch(0.24 0.03 70)',
	popover: 'oklch(1 0 0)',
	'popover-foreground': 'oklch(0.24 0.03 70)',
	primary: 'oklch(0.8 0.17 85)',
	'primary-foreground': 'oklch(0.25 0.05 70)',
	secondary: 'oklch(0.96 0.02 92)',
	'secondary-foreground': 'oklch(0.3 0.04 70)',
	muted: 'oklch(0.965 0.015 92)',
	'muted-foreground': 'oklch(0.5 0.04 75)',
	accent: 'oklch(0.95 0.07 92)',
	'accent-foreground': 'oklch(0.3 0.06 70)',
	destructive: 'oklch(0.55 0.21 25)',
	border: 'oklch(0.91 0.025 90)',
	input: 'oklch(0.89 0.03 90)',
	ring: 'oklch(0.75 0.16 85)',
	sidebar: 'oklch(0.26 0.045 60)',
	'sidebar-foreground': 'oklch(0.93 0.015 90)',
	'sidebar-primary': 'oklch(0.82 0.17 87)',
	'sidebar-primary-foreground': 'oklch(0.25 0.05 70)',
	'sidebar-accent': 'oklch(0.34 0.05 62)',
	'sidebar-accent-foreground': 'oklch(0.97 0.01 90)',
	'sidebar-border': 'oklch(0.36 0.045 62)',
	'sidebar-ring': 'oklch(0.82 0.17 87)'
};

const dark: ThemeTokenMap = {
	background: 'oklch(0.18 0.015 70)',
	foreground: 'oklch(0.95 0.01 90)',
	card: 'oklch(0.22 0.018 70)',
	'card-foreground': 'oklch(0.95 0.01 90)',
	popover: 'oklch(0.25 0.02 70)',
	'popover-foreground': 'oklch(0.95 0.01 90)',
	primary: 'oklch(0.83 0.17 87)',
	'primary-foreground': 'oklch(0.22 0.04 70)',
	secondary: 'oklch(0.28 0.02 70)',
	'secondary-foreground': 'oklch(0.95 0.01 90)',
	muted: 'oklch(0.28 0.02 70)',
	'muted-foreground': 'oklch(0.73 0.03 85)',
	accent: 'oklch(0.33 0.06 85)',
	'accent-foreground': 'oklch(0.95 0.02 90)',
	destructive: 'oklch(0.65 0.19 25)',
	border: 'oklch(1 0 0 / 9%)',
	input: 'oklch(1 0 0 / 12%)',
	ring: 'oklch(0.78 0.15 85)',
	sidebar: 'oklch(0.14 0.02 65)',
	'sidebar-foreground': 'oklch(0.93 0.015 90)',
	'sidebar-primary': 'oklch(0.83 0.17 87)',
	'sidebar-primary-foreground': 'oklch(0.22 0.04 70)',
	'sidebar-accent': 'oklch(0.24 0.035 65)',
	'sidebar-accent-foreground': 'oklch(0.97 0.01 90)',
	'sidebar-border': 'oklch(1 0 0 / 9%)',
	'sidebar-ring': 'oklch(0.83 0.17 87)'
};

/** Built-in enterprise default. */
export const enterpriseTheme: ThemeConfig = {
	name: 'enterprise',
	label: 'Enterprise',
	colorMode: 'system',
	source: 'fallback',
	layout: 'default',
	errorPages: 'default',
	nav: null,
	tokens: {
		light,
		dark,
		radius: '0.5rem',
		fontSans: '"Noto Sans Variable", "Segoe UI", system-ui, sans-serif'
	}
};

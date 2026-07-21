import type { ThemeConfig, ThemeTokenMap } from './types';

const light: ThemeTokenMap = {
	background: 'oklch(0.985 0.002 247)',
	foreground: 'oklch(0.22 0.02 255)',
	card: 'oklch(1 0 0)',
	'card-foreground': 'oklch(0.22 0.02 255)',
	popover: 'oklch(1 0 0)',
	'popover-foreground': 'oklch(0.22 0.02 255)',
	primary: 'oklch(0.45 0.14 255)',
	'primary-foreground': 'oklch(0.99 0.01 255)',
	secondary: 'oklch(0.94 0.01 255)',
	'secondary-foreground': 'oklch(0.28 0.03 255)',
	muted: 'oklch(0.95 0.008 250)',
	'muted-foreground': 'oklch(0.48 0.02 255)',
	accent: 'oklch(0.93 0.02 250)',
	'accent-foreground': 'oklch(0.28 0.03 255)',
	destructive: 'oklch(0.55 0.2 25)',
	border: 'oklch(0.88 0.01 255)',
	input: 'oklch(0.88 0.01 255)',
	ring: 'oklch(0.55 0.12 255)',
	sidebar: 'oklch(0.24 0.03 255)',
	'sidebar-foreground': 'oklch(0.93 0.01 255)',
	'sidebar-primary': 'oklch(0.62 0.14 255)',
	'sidebar-primary-foreground': 'oklch(0.99 0.01 255)',
	'sidebar-accent': 'oklch(0.32 0.03 255)',
	'sidebar-accent-foreground': 'oklch(0.95 0.01 255)',
	'sidebar-border': 'oklch(0.34 0.02 255)',
	'sidebar-ring': 'oklch(0.62 0.14 255)'
};

const dark: ThemeTokenMap = {
	background: 'oklch(0.18 0.02 255)',
	foreground: 'oklch(0.95 0.01 255)',
	card: 'oklch(0.22 0.02 255)',
	'card-foreground': 'oklch(0.95 0.01 255)',
	popover: 'oklch(0.22 0.02 255)',
	'popover-foreground': 'oklch(0.95 0.01 255)',
	primary: 'oklch(0.68 0.13 255)',
	'primary-foreground': 'oklch(0.18 0.02 255)',
	secondary: 'oklch(0.28 0.02 255)',
	'secondary-foreground': 'oklch(0.95 0.01 255)',
	muted: 'oklch(0.28 0.02 255)',
	'muted-foreground': 'oklch(0.7 0.02 255)',
	accent: 'oklch(0.3 0.03 255)',
	'accent-foreground': 'oklch(0.95 0.01 255)',
	destructive: 'oklch(0.65 0.18 25)',
	border: 'oklch(1 0 0 / 12%)',
	input: 'oklch(1 0 0 / 14%)',
	ring: 'oklch(0.58 0.12 255)',
	sidebar: 'oklch(0.16 0.025 255)',
	'sidebar-foreground': 'oklch(0.93 0.01 255)',
	'sidebar-primary': 'oklch(0.68 0.13 255)',
	'sidebar-primary-foreground': 'oklch(0.16 0.025 255)',
	'sidebar-accent': 'oklch(0.24 0.03 255)',
	'sidebar-accent-foreground': 'oklch(0.95 0.01 255)',
	'sidebar-border': 'oklch(1 0 0 / 10%)',
	'sidebar-ring': 'oklch(0.68 0.13 255)'
};

/** Built-in enterprise fallback — M365 / Oracle inspired cool neutrals + trust blue. */
export const enterpriseTheme: ThemeConfig = {
	name: 'enterprise',
	label: 'Enterprise',
	colorMode: 'system',
	source: 'fallback',
	tokens: {
		light,
		dark,
		radius: '0.375rem',
		fontSans: '"Noto Sans Variable", "Segoe UI", system-ui, sans-serif'
	}
};

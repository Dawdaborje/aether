import type { ThemeConfig, ThemeTokenMap } from './types';

/*
 * The built-in default theme: what the app looks like when an organization has no
 * theme plugin installed. Cool neutral greys, a single restrained blue for actions and
 * focus, a dark graphite top bar, white work surfaces on a light grey canvas, hairline
 * borders and tight corners. `routes/layout.css` and the server's `api/theme.rs` carry
 * the same values so the first paint (before any script runs) already matches.
 */
const light: ThemeTokenMap = {
	background: 'oklch(0.971 0.006 185)',
	foreground: 'oklch(0.229 0.022 191)',
	card: 'oklch(1.000 0.000 90)',
	'card-foreground': 'oklch(0.229 0.022 191)',
	popover: 'oklch(1.000 0.000 90)',
	'popover-foreground': 'oklch(0.229 0.022 191)',
	primary: 'oklch(0.511 0.086 186)',
	'primary-foreground': 'oklch(0.981 0.010 189)',
	secondary: 'oklch(0.947 0.011 183)',
	'secondary-foreground': 'oklch(0.345 0.047 189)',
	muted: 'oklch(0.955 0.009 180)',
	'muted-foreground': 'oklch(0.510 0.025 189)',
	accent: 'oklch(0.940 0.021 182)',
	'accent-foreground': 'oklch(0.387 0.063 187)',
	destructive: 'oklch(0.55 0.21 25)',
	border: 'oklch(0.917 0.014 181)',
	input: 'oklch(0.890 0.018 179)',
	ring: 'oklch(0.604 0.103 185)',
	highlight: 'oklch(0.769 0.165 70)',
	'highlight-foreground': 'oklch(0.234 0.049 76)',
	sidebar: 'oklch(0.330 0.048 197)',
	'sidebar-foreground': 'oklch(0.912 0.023 186)',
	'sidebar-primary': 'oklch(0.769 0.165 70)',
	'sidebar-primary-foreground': 'oklch(0.234 0.049 76)',
	'sidebar-accent': 'oklch(0.411 0.060 205)',
	'sidebar-accent-foreground': 'oklch(0.981 0.010 189)',
	'sidebar-border': 'oklch(0.273 0.039 198)',
	'sidebar-ring': 'oklch(0.769 0.165 70)'
};

const dark: ThemeTokenMap = {
	background: 'oklch(0.194 0.016 196)',
	foreground: 'oklch(0.949 0.012 184)',
	card: 'oklch(0.234 0.021 191)',
	'card-foreground': 'oklch(0.949 0.012 184)',
	popover: 'oklch(0.264 0.025 191)',
	'popover-foreground': 'oklch(0.949 0.012 184)',
	primary: 'oklch(0.785 0.133 182)',
	'primary-foreground': 'oklch(0.280 0.046 186)',
	secondary: 'oklch(0.285 0.025 192)',
	'secondary-foreground': 'oklch(0.949 0.012 184)',
	muted: 'oklch(0.285 0.025 192)',
	'muted-foreground': 'oklch(0.714 0.029 188)',
	accent: 'oklch(0.338 0.039 188)',
	'accent-foreground': 'oklch(0.949 0.012 184)',
	destructive: 'oklch(0.65 0.19 25)',
	border: 'oklch(1 0 0 / 9%)',
	input: 'oklch(1 0 0 / 12%)',
	ring: 'oklch(0.785 0.133 182)',
	highlight: 'oklch(0.837 0.164 84)',
	'highlight-foreground': 'oklch(0.234 0.049 76)',
	sidebar: 'oklch(0.221 0.030 195)',
	'sidebar-foreground': 'oklch(0.905 0.024 187)',
	'sidebar-primary': 'oklch(0.837 0.164 84)',
	'sidebar-primary-foreground': 'oklch(0.234 0.049 76)',
	'sidebar-accent': 'oklch(0.321 0.044 197)',
	'sidebar-accent-foreground': 'oklch(0.981 0.010 189)',
	'sidebar-border': 'oklch(1 0 0 / 8%)',
	'sidebar-ring': 'oklch(0.837 0.164 84)'
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
		radius: '0.25rem',
		fontSans: '"Noto Sans Variable", "Segoe UI", system-ui, sans-serif'
	}
};

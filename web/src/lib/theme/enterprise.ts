import type { ThemeConfig, ThemeTokenMap } from './types';

/*
 * The built-in default theme: what the app looks like when an organization has no
 * theme plugin installed. Cool neutral greys, a single restrained blue for actions and
 * focus, a dark graphite top bar, white work surfaces on a light grey canvas, hairline
 * borders and tight corners. `routes/layout.css` and the server's `api/theme.rs` carry
 * the same values so the first paint (before any script runs) already matches.
 */
const light: ThemeTokenMap = {
	background: 'oklch(0.971 0.006 250)',
	foreground: 'oklch(0.229 0.022 256)',
	card: 'oklch(1.000 0.000 90)',
	'card-foreground': 'oklch(0.229 0.022 256)',
	popover: 'oklch(1.000 0.000 90)',
	'popover-foreground': 'oklch(0.229 0.022 256)',
	primary: 'oklch(0.511 0.086 250)',
	'primary-foreground': 'oklch(0.981 0.010 254)',
	secondary: 'oklch(0.947 0.011 248)',
	'secondary-foreground': 'oklch(0.345 0.047 254)',
	muted: 'oklch(0.955 0.009 245)',
	'muted-foreground': 'oklch(0.510 0.025 254)',
	accent: 'oklch(0.940 0.021 247)',
	'accent-foreground': 'oklch(0.387 0.063 252)',
	destructive: 'oklch(0.55 0.21 25)',
	border: 'oklch(0.917 0.014 246)',
	input: 'oklch(0.890 0.018 244)',
	ring: 'oklch(0.604 0.103 250)',
	highlight: 'oklch(0.769 0.165 70)',
	'highlight-foreground': 'oklch(0.234 0.049 76)',
	sidebar: 'oklch(0.330 0.048 262)',
	'sidebar-foreground': 'oklch(0.912 0.023 251)',
	'sidebar-primary': 'oklch(0.769 0.165 70)',
	'sidebar-primary-foreground': 'oklch(0.234 0.049 76)',
	'sidebar-accent': 'oklch(0.411 0.060 270)',
	'sidebar-accent-foreground': 'oklch(0.981 0.010 254)',
	'sidebar-border': 'oklch(0.273 0.039 263)',
	'sidebar-ring': 'oklch(0.769 0.165 70)'
};

const dark: ThemeTokenMap = {
	background: 'oklch(0.194 0.016 261)',
	foreground: 'oklch(0.949 0.012 249)',
	card: 'oklch(0.234 0.021 256)',
	'card-foreground': 'oklch(0.949 0.012 249)',
	popover: 'oklch(0.264 0.025 256)',
	'popover-foreground': 'oklch(0.949 0.012 249)',
	primary: 'oklch(0.785 0.133 250)',
	'primary-foreground': 'oklch(0.280 0.046 251)',
	secondary: 'oklch(0.285 0.025 257)',
	'secondary-foreground': 'oklch(0.949 0.012 249)',
	muted: 'oklch(0.285 0.025 257)',
	'muted-foreground': 'oklch(0.714 0.029 253)',
	accent: 'oklch(0.338 0.039 253)',
	'accent-foreground': 'oklch(0.949 0.012 249)',
	destructive: 'oklch(0.65 0.19 25)',
	border: 'oklch(1 0 0 / 9%)',
	input: 'oklch(1 0 0 / 12%)',
	ring: 'oklch(0.785 0.133 250)',
	highlight: 'oklch(0.837 0.164 84)',
	'highlight-foreground': 'oklch(0.234 0.049 76)',
	sidebar: 'oklch(0.221 0.030 260)',
	'sidebar-foreground': 'oklch(0.905 0.024 252)',
	'sidebar-primary': 'oklch(0.837 0.164 84)',
	'sidebar-primary-foreground': 'oklch(0.234 0.049 76)',
	'sidebar-accent': 'oklch(0.321 0.044 262)',
	'sidebar-accent-foreground': 'oklch(0.981 0.010 254)',
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
	layoutDef: null,
	errorPagesDef: null,
	tokens: {
		light,
		dark,
		radius: '0.5rem',
		fontSans: '"Noto Sans Variable", "Segoe UI", system-ui, sans-serif'
	}
};

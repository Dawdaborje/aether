import {
	SHAPE_ROLES,
	SHAPE_STEPS,
	TYPE_ROLES,
	TYPE_STEPS,
	ELEVATION_STEPS,
	type ThemeApps,
	type ThemeElevation,
	type ThemeFocus,
	type ThemeMotion,
	type ThemeShape,
	type ThemeType,
	type ThemeConfig,
	type ThemeTokenMap
} from './types';

/**
 * Tokens the built-in Carbon theme added. A theme that leaves them out gets them derived
 * from its own colours, not Carbon's values from `layout.css`.
 */
export const DERIVED_TOKENS: ThemeTokenMap = {
	field: 'var(--card)',
	'field-hover': 'color-mix(in oklab, var(--foreground) 6%, var(--card))',
	'layer-hover': 'color-mix(in oklab, var(--foreground) 6%, var(--card))',
	'border-strong': 'var(--input)',
	'primary-hover': 'color-mix(in oklab, var(--primary) 88%, black)',
	'secondary-hover': 'color-mix(in oklab, var(--secondary) 92%, var(--foreground))'
};

/**
 * Apply a theme: both palettes go into one stylesheet (`:root` for light, `.dark` for
 * dark). They must not be inline styles on <html>: those would beat the `.dark` rule and
 * leave dark mode half light.
 */
export function applyTheme(theme: ThemeConfig) {
	if (typeof document === 'undefined') return;

	const root = document.documentElement;
	// Clear anything an earlier version of the app wrote inline.
	for (const key of [
		...Object.keys(theme.tokens.light),
		'radius',
		'font-sans',
		'font-mono',
		'font-heading'
	]) {
		root.style.removeProperty(`--${key}`);
	}

	const styleId = 'aether-theme';
	let style = document.getElementById(styleId) as HTMLStyleElement | null;
	if (!style) {
		style = document.createElement('style');
		style.id = styleId;
		document.head.appendChild(style);
	}

	const rules = (tokens: ThemeTokenMap) =>
		Object.entries({ ...DERIVED_TOKENS, ...tokens })
			.map(([key, value]) => `--${key}: ${value};`)
			.join('');
	const { radius, fontSans, fontMono, fontHeading } = theme.tokens;
	const shared = [
		`--radius: ${radius};`,
		`--font-sans: ${fontSans};`,
		fontMono ? `--font-mono: ${fontMono};` : '',
		shapeRules(theme.tokens.shape),
		typeRules(theme.tokens.type),
		elevationRules(theme.tokens.elevation),
		motionRules(theme.tokens.motion),
		// Headings use the body font unless the theme names one.
		`--font-heading: ${fontHeading || 'var(--font-sans)'};`
	].join(' ');
	style.textContent = `:root { ${rules(theme.tokens.light)} ${shared} } .dark { ${rules(theme.tokens.dark)} ${shared} } ${motionReduced()} ${focusRules(theme.tokens.focus)} ${appRules(theme.tokens.apps)}`;

	// Spacing scale: `layout.css` reads `[data-density]`.
	root.dataset.density = theme.tokens.density ?? 'comfortable';
	// Field and table looks: `layout.css` reads these attributes.
	const looks: Record<string, string | undefined> = {
		field: theme.tokens.field?.style,
		'table-header': theme.tokens.table?.header,
		'table-rows': theme.tokens.table?.rows,
		'table-density': theme.tokens.table?.density
	};
	for (const [name, value] of Object.entries(looks)) {
		if (value) root.dataset[name.replace(/-(\w)/g, (_, c: string) => c.toUpperCase())] = value;
		else delete root.dataset[name.replace(/-(\w)/g, (_, c: string) => c.toUpperCase())];
	}
	applyFavicon(theme.shell?.branding?.favicon ?? null);
	applyStylesheet(theme.shell?.css ?? '');
}

/**
 * `--shape-<step>` and `--shape-<role>` for what the theme sets; `layout.css` defines the rest
 * (the scale from `--radius`, each role from a step) and maps `rounded-*` onto them. A role
 * names a step, `none`, `full` or a length.
 */
export function shapeRules(shape: ThemeShape | undefined): string {
	if (!shape) return '';
	const steps = SHAPE_STEPS.filter((step) => shape.scale?.[step]).map(
		(step) => `--shape-${step}: ${shape.scale![step]};`
	);
	// A role is one corner value, or up to four (top-left, top-right, bottom-right, bottom-left).
	const corner = (value: string) =>
		(SHAPE_STEPS as readonly string[]).includes(value)
			? `var(--shape-${value})`
			: value === 'none'
				? '0'
				: value === 'full'
					? 'calc(infinity * 1px)'
					: value;
	const roles = SHAPE_ROLES.filter((role) => shape.roles?.[role]).map(
		(role) =>
			`--shape-${role}: ${shape.roles![role]!.split(/\s+/).filter(Boolean).map(corner).join(' ')};`
	);
	return [...steps, ...roles].join(' ');
}

const TYPE_FAMILIES = {
	sans: 'var(--font-sans)',
	heading: 'var(--font-heading)',
	mono: 'var(--font-mono)'
};
const TYPE_CASES = {
	none: 'none',
	upper: 'uppercase',
	lower: 'lowercase',
	capitalize: 'capitalize'
};

/**
 * `--text-<step>` for the scale steps the theme sets (Tailwind's `text-<step>` reads them), and
 * `--type-<role>-<property>` for what it sets on each role; `layout.css` defines the rest and
 * the `type-<role>` utilities. A role's `size` names a step or a length.
 */
export function typeRules(type: ThemeType | undefined): string {
	if (!type) return '';
	const steps = TYPE_STEPS.filter((step) => type.scale?.[step]).map(
		(step) => `--text-${step}: ${type.scale![step]};`
	);
	const roles = TYPE_ROLES.flatMap((role) => {
		const spec = type.roles?.[role];
		if (!spec) return [];
		const out: string[] = [];
		const set = (property: string, value: string | number | undefined) => {
			if (value !== undefined && value !== '') out.push(`--type-${role}-${property}: ${value};`);
		};
		set(
			'size',
			spec.size && (TYPE_STEPS as readonly string[]).includes(spec.size)
				? `var(--text-${spec.size})`
				: spec.size
		);
		set('leading', spec.line_height);
		set('weight', spec.weight);
		set('tracking', spec.tracking);
		set('family', spec.family && TYPE_FAMILIES[spec.family]);
		set('case', spec.case && TYPE_CASES[spec.case]);
		return out;
	});
	return [...steps, ...roles].join(' ');
}

/**
 * The theme's own stylesheet, after the stock styles so it wins. The server has already
 * refused anything that fetches or runs; text content cannot add markup either way.
 */
function applyStylesheet(css: string) {
	const id = 'aether-theme-css';
	let style = document.getElementById(id) as HTMLStyleElement | null;
	if (!css) {
		style?.remove();
		return;
	}
	if (!style) {
		style = document.createElement('style');
		style.id = id;
		document.head.appendChild(style);
	}
	style.textContent = css;
}

/** The theme's favicon, or the app's own (`static/favicon`) when it has none. */
function applyFavicon(href: string | null) {
	let link = document.querySelector<HTMLLinkElement>('link[rel="icon"][data-theme-icon]');
	if (!href) {
		link?.remove();
		return;
	}
	if (!link) {
		link = document.createElement('link');
		link.rel = 'icon';
		link.dataset.themeIcon = '';
		document.head.appendChild(link);
	}
	link.href = href;
}

export function resolveColorMode(mode: ThemeConfig['colorMode']): 'light' | 'dark' {
	if (mode === 'light' || mode === 'dark') return mode;
	if (typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches) {
		return 'dark';
	}
	return 'light';
}

export function applyColorMode(mode: ThemeConfig['colorMode']) {
	if (typeof document === 'undefined') return;
	const resolved = resolveColorMode(mode);
	const root = document.documentElement;
	root.classList.toggle('dark', resolved === 'dark');
	root.style.colorScheme = resolved;
}

/** `--elevation-<step>` for what the theme sets; `layout.css` maps `shadow-<step>` onto them. */
export function elevationRules(elevation: ThemeElevation | undefined): string {
	if (!elevation) return '';
	return ELEVATION_STEPS.filter((step) => elevation[step])
		.map((step) => `--elevation-${step}: ${elevation[step]};`)
		.join(' ');
}

/** `--motion-<key>` for what the theme sets; `duration-short`/`-medium` utilities read them. */
export function motionRules(motion: ThemeMotion | undefined): string {
	if (!motion) return '';
	return Object.entries(motion)
		.filter(([, value]) => value)
		.map(([key, value]) => `--motion-${key}: ${value};`)
		.join(' ');
}

/** People who ask for less motion get none, whatever the theme sets. */
export function motionReduced(): string {
	return '@media (prefers-reduced-motion: reduce) { :root { --motion-duration-short: 0ms; --motion-duration-medium: 0ms; } }';
}

/** The focus indicator, drawn on every focusable element when the theme sets one. */
export function focusRules(focus: ThemeFocus | undefined): string {
	if (!focus) return '';
	const width = focus.width ?? '2px';
	const offset = focus.offset ?? '2px';
	const target =
		':where(a, button, input, select, textarea, summary, [tabindex], [role="button"]):focus-visible';
	const base = `${target} { outline: ${width} solid var(--ring); outline-offset: ${offset}; box-shadow: none !important; }`;
	return focus.style === 'fill'
		? `${base} ${target} { background-color: var(--focus-fill, var(--primary)); color: var(--focus-text, var(--primary-foreground)); outline-offset: 0; }`
		: base;
}

/** Per-app colours: the shell sets `data-app` on the root while an app is open. */
export function appRules(apps: ThemeApps | undefined): string {
	if (!apps) return '';
	return Object.entries(apps)
		.map(([app, colours]) => {
			const body = [
				colours.primary && `--primary: ${colours.primary}; --ring: ${colours.primary};`,
				colours.header && `--header: ${colours.header};`,
				colours.header_foreground && `--header-foreground: ${colours.header_foreground};`
			]
				.filter(Boolean)
				.join(' ');
			return body ? `:root[data-app="${app}"] { ${body} }` : '';
		})
		.join(' ');
}

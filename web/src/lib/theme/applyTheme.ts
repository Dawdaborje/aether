import type { ThemeConfig, ThemeTokenMap } from './types';

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
		Object.entries(tokens)
			.map(([key, value]) => `--${key}: ${value};`)
			.join('');
	const { radius, fontSans, fontMono, fontHeading } = theme.tokens;
	const shared = [
		`--radius: ${radius};`,
		`--font-sans: ${fontSans};`,
		fontMono ? `--font-mono: ${fontMono};` : '',
		// Headings use the body font unless the theme names one.
		`--font-heading: ${fontHeading || 'var(--font-sans)'};`
	].join(' ');
	style.textContent = `:root { ${rules(theme.tokens.light)} ${shared} } .dark { ${rules(theme.tokens.dark)} ${shared} }`;

	// Spacing scale: `layout.css` reads `[data-density]`.
	root.dataset.density = theme.tokens.density ?? 'comfortable';
	applyFavicon(theme.shell?.branding?.favicon ?? null);
	applyStylesheet(theme.shell?.css ?? '');
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

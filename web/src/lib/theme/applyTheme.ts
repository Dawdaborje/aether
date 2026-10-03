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
	for (const key of [...Object.keys(theme.tokens.light), 'radius', 'font-sans']) {
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
	const shared = `--radius: ${theme.tokens.radius}; --font-sans: ${theme.tokens.fontSans};`;
	style.textContent = `:root { ${rules(theme.tokens.light)} ${shared} } .dark { ${rules(theme.tokens.dark)} ${shared} }`;
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
	document.documentElement.classList.toggle('dark', resolved === 'dark');
}

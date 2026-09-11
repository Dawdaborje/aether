import type { ThemeConfig, ThemeTokenMap } from './types';

function setVars(el: HTMLElement, tokens: ThemeTokenMap, prefix = '') {
	for (const [key, value] of Object.entries(tokens)) {
		el.style.setProperty(`--${prefix}${key}`, value);
	}
}

/** Apply theme CSS variables to :root (light) and prepare dark via class-scoped vars. */
export function applyTheme(theme: ThemeConfig) {
	if (typeof document === 'undefined') return;

	const root = document.documentElement;
	setVars(root, theme.tokens.light);
	root.style.setProperty('--radius', theme.tokens.radius);
	root.style.setProperty('--font-sans', theme.tokens.fontSans);

	// Persist dark tokens on a style tag so `.dark` can resolve them.
	const styleId = 'aether-theme-dark';
	let style = document.getElementById(styleId) as HTMLStyleElement | null;
	if (!style) {
		style = document.createElement('style');
		style.id = styleId;
		document.head.appendChild(style);
	}

	const darkRules = Object.entries(theme.tokens.dark)
		.map(([key, value]) => `--${key}: ${value};`)
		.join('');
	style.textContent = `.dark { ${darkRules} --radius: ${theme.tokens.radius}; --font-sans: ${theme.tokens.fontSans}; }`;
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

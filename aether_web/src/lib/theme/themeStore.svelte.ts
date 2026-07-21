import { applyColorMode, applyTheme } from './applyTheme';
import { enterpriseTheme } from './enterprise';
import type { ThemeApiResponse, ThemeConfig, ThemeColorMode } from './types';

function fromApi(payload: ThemeApiResponse): ThemeConfig {
	const mode = (['light', 'dark', 'system'].includes(payload.color_mode)
		? payload.color_mode
		: 'system') as ThemeColorMode;

	return {
		name: payload.name,
		label: payload.label,
		colorMode: mode,
		source: payload.source === 'api' ? 'api' : 'fallback',
		tokens: {
			light: payload.tokens.light,
			dark: payload.tokens.dark,
			radius: payload.tokens.radius,
			fontSans: payload.tokens.font_sans
		}
	};
}

class ThemeStore {
	theme = $state<ThemeConfig>(enterpriseTheme);
	loading = $state(false);
	error = $state<string | null>(null);

	constructor() {
		applyTheme(this.theme);
		applyColorMode(this.theme.colorMode);
	}

	setTheme(theme: ThemeConfig) {
		this.theme = theme;
		applyTheme(theme);
		applyColorMode(theme.colorMode);
	}

	async loadFromApi(baseUrl = '') {
		this.loading = true;
		this.error = null;
		try {
			const res = await fetch(`${baseUrl}/api/ui/theme`);
			if (!res.ok) throw new Error(`theme API ${res.status}`);
			const data = (await res.json()) as ThemeApiResponse;
			this.setTheme(fromApi(data));
		} catch (err) {
			this.error = err instanceof Error ? err.message : 'Failed to load theme';
			this.setTheme(enterpriseTheme);
		} finally {
			this.loading = false;
		}
	}
}

export const themeStore = new ThemeStore();

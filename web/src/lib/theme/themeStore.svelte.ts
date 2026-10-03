import { apiFetch } from '$lib/api/client';
import { applyColorMode, applyTheme, resolveColorMode } from './applyTheme';
import { enterpriseTheme } from './enterprise';
import type { ThemeApiResponse, ThemeConfig, ThemeColorMode } from './types';

function fromApi(payload: ThemeApiResponse): ThemeConfig {
	// The server says "fallback" when the organization has no theme: the built-in
	// enterprise theme (baked into the app) is used, whatever tokens came with it.
	if (payload.source !== 'organization') return enterpriseTheme;

	const mode = (
		['light', 'dark', 'system'].includes(payload.color_mode) ? payload.color_mode : 'system'
	) as ThemeColorMode;

	return {
		name: payload.name,
		label: payload.label,
		colorMode: mode,
		source: payload.source === 'organization' ? 'organization' : 'fallback',
		tokens: {
			light: payload.tokens.light,
			dark: payload.tokens.dark,
			radius: payload.tokens.radius,
			fontSans: payload.tokens.font_sans
		},
		layout: payload.layout || 'default',
		errorPages: payload.error_pages || 'default',
		nav: payload.nav ?? null
	};
}

const MODE_KEY = 'aether-color-mode';

function readStoredMode(): 'light' | 'dark' | null {
	try {
		const v = localStorage.getItem(MODE_KEY);
		return v === 'light' || v === 'dark' ? v : null;
	} catch {
		return null;
	}
}

class ThemeStore {
	theme = $state<ThemeConfig>(enterpriseTheme);
	/** The viewer's own choice; null follows the theme's mode (system by default). */
	userMode = $state<'light' | 'dark' | null>(null);
	isDark = $state(false);
	loading = $state(false);
	error = $state<string | null>(null);

	constructor() {
		if (typeof window !== 'undefined') this.userMode = readStoredMode();
		applyTheme(this.theme);
		this.syncMode();
		if (typeof window !== 'undefined') {
			// While following the system, track OS changes live.
			window
				.matchMedia('(prefers-color-scheme: dark)')
				.addEventListener('change', () => this.syncMode());
		}
	}

	private syncMode() {
		applyColorMode(this.userMode ?? this.theme.colorMode);
		this.isDark = resolveColorMode(this.userMode ?? this.theme.colorMode) === 'dark';
	}

	/** Set the viewer's preference; null goes back to the system setting. */
	setUserMode(mode: 'light' | 'dark' | null) {
		this.userMode = mode;
		try {
			if (mode) localStorage.setItem(MODE_KEY, mode);
			else localStorage.removeItem(MODE_KEY);
		} catch {
			/* storage unavailable: the choice lasts for this session only */
		}
		this.syncMode();
	}

	setTheme(theme: ThemeConfig) {
		this.theme = theme;
		applyTheme(theme);
		this.syncMode();
	}

	private inflight: Promise<void> | null = null;

	/** Load the theme; callers that ask while a load is running share it. */
	loadFromApi(baseUrl = ''): Promise<void> {
		this.inflight ??= this.fetchTheme(baseUrl).finally(() => {
			this.inflight = null;
		});
		return this.inflight;
	}

	private async fetchTheme(baseUrl: string) {
		this.loading = true;
		this.error = null;
		try {
			const res = await apiFetch(`${baseUrl}/api/ui/theme`);
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

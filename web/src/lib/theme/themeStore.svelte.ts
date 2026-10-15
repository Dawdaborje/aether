import { browserStorage } from '$lib/browserStorage';
import { apiFetch } from '$lib/api/client';
import { applyColorMode, applyTheme, resolveColorMode } from './applyTheme';
import { defaultTheme } from './default';
import type { ThemeApiResponse, ThemeConfig, ThemeColorMode } from './types';

function fromApi(payload: ThemeApiResponse): ThemeConfig {
	// The server says "fallback" when the organization has no theme: the built-in
	// default theme (baked into the app) is used, whatever tokens came with it.
	if (payload.source !== 'organization') return defaultTheme;

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
			fontSans: payload.tokens.font_sans,
			fontMono: payload.tokens.font_mono,
			fontHeading: payload.tokens.font_heading,
			density: (['compact', 'comfortable', 'spacious'] as const).find(
				(d) => d === payload.tokens.density
			)
		},
		layout: payload.layout || 'default',
		errorPages: payload.error_pages || 'default',
		nav: payload.nav ?? null,
		layoutDef: payload.layout_def ?? null,
		errorPagesDef: payload.error_pages_def ?? null,
		shell: payload.shell ?? null
	};
}

const MODE_KEY = 'aether-color-mode';

function readStoredMode(): 'light' | 'dark' | null {
	try {
		const v = browserStorage()?.getItem(MODE_KEY);
		return v === 'light' || v === 'dark' ? v : null;
	} catch {
		return null;
	}
}

class ThemeStore {
	theme = $state<ThemeConfig>(defaultTheme);
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
			// After sleep/unlock the OS preference can flap; re-assert our stored choice.
			document.addEventListener('visibilitychange', () => {
				if (document.visibilityState === 'visible') this.syncMode();
			});
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
			const storage = browserStorage();
			if (mode) storage?.setItem(MODE_KEY, mode);
			else storage?.removeItem(MODE_KEY);
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
			this.setTheme(defaultTheme);
		} finally {
			this.loading = false;
		}
	}
}

export const themeStore = new ThemeStore();

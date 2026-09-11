export type ThemeColorMode = 'light' | 'dark' | 'system';

export type ThemeTokenMap = Record<string, string>;

export interface ThemeTokens {
	light: ThemeTokenMap;
	dark: ThemeTokenMap;
	radius: string;
	fontSans: string;
}

export interface ThemeConfig {
	name: string;
	label: string;
	colorMode: ThemeColorMode;
	source: 'api' | 'fallback';
	tokens: ThemeTokens;
}

export interface ThemeApiResponse {
	name: string;
	label: string;
	color_mode: string;
	source: string;
	tokens: {
		light: ThemeTokenMap;
		dark: ThemeTokenMap;
		radius: string;
		font_sans: string;
	};
}

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { defaultTheme } from '$lib/theme/default';
import { themeStore } from '$lib/theme/themeStore.svelte';
import { defaultIcon, defaultIcons, iconRef, themeIcon } from './index';

const bell = { view_box: '0 0 24 24', attrs: { fill: 'none' }, body: '<path d="M6 16v-5"/>' };

describe('theme icons', () => {
	afterEach(() => {
		themeStore.theme = defaultTheme;
	});

	it('uses the built-in icons when the theme ships none', () => {
		expect(themeIcon('bell')).toBeUndefined();
		expect(iconRef('bell')).toBe('bell');
		expect(iconRef('no-such-icon')).toBeUndefined();
		expect(iconRef(null)).toBeUndefined();
	});

	it("prefers the theme's sprite and adds the names it defines", () => {
		themeStore.theme = {
			...defaultTheme,
			shell: { icons: { bell, 'deal-stage': bell } }
		};
		expect(themeIcon('bell')).toEqual(bell);
		expect(iconRef('deal-stage')).toBe('deal-stage');
		expect(defaultIcon('deal-stage')).toBeUndefined();
	});

	it('ignores names inherited from Object', () => {
		themeStore.theme = { ...defaultTheme, shell: { icons: { bell } } };
		expect(themeIcon('constructor')).toBeUndefined();
		expect(defaultIcon('toString')).toBeUndefined();
		expect(iconRef('hasOwnProperty')).toBeUndefined();
	});
});

describe('icon names in the code', () => {
	const sources = import.meta.glob('/src/**/*.svelte', {
		query: '?raw',
		import: 'default',
		eager: true
	}) as Record<string, string>;

	it('every literal name is in the default set', () => {
		const unknown: string[] = [];
		let seen = 0;
		for (const [file, source] of Object.entries(sources)) {
			const names = [
				...[...source.matchAll(/<AppIcon\b[^>]*?\sname="([^"]+)"/g)].map((m) => m[1]),
				...[...source.matchAll(/<(?:Empty|SearchPicker)\b[^>]*?\sicon="([^"]+)"/g)].map((m) => m[1])
			];
			seen += names.length;
			for (const name of names) if (!defaultIcon(name)) unknown.push(`${file}: ${name}`);
		}
		expect(seen).toBeGreaterThan(200);
		expect(unknown).toEqual([]);
	});

	it('nothing draws a Carbon icon directly', () => {
		const direct = Object.entries(sources)
			.filter(([, source]) => source.includes('carbon-icons-svelte'))
			.map(([file]) => file);
		expect(direct).toEqual([]);
	});
});

describe('the icon names in the docs', () => {
	it('match the default set', () => {
		const docs = readFileSync(
			fileURLToPath(new URL('../../../../docs/architecture/themes.md', import.meta.url)),
			'utf8'
		);
		const block = docs.slice(
			docs.indexOf('<!-- icon-names:start -->'),
			docs.indexOf('<!-- icon-names:end -->')
		);
		const listed = [...block.matchAll(/`([a-z0-9-]+)`/g)].map((m) => m[1]).sort();
		expect(listed).toEqual(Object.keys(defaultIcons).sort());
	});
});

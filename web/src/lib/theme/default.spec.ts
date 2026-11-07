import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import manifest from './default-theme.json';

/** `--name: value;` pairs of the first `selector { … }` rule in `css`. */
function tokens(css: string, selector: string): Record<string, string> {
	const start = css.indexOf(`${selector} {`);
	const body = css.slice(start, css.indexOf('}', start));
	return Object.fromEntries(
		[...body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)].map(([, name, value]) => [name, value.trim()])
	);
}

const css = readFileSync(
	fileURLToPath(new URL('../../routes/layout.css', import.meta.url)),
	'utf8'
);

describe('default theme', () => {
	// layout.css paints before any script runs; it must match the manifest the app and
	// the server use, or the page changes colour once the theme is applied.
	it('layout.css repeats default-theme.json', () => {
		const light = tokens(css, ':root');
		const dark = tokens(css, '.dark');
		for (const [name, value] of Object.entries(manifest.light)) {
			expect(light[name], `light --${name}`).toBe(value);
		}
		for (const [name, value] of Object.entries(manifest.dark)) {
			expect(dark[name], `dark --${name}`).toBe(value);
		}
		expect(light.radius).toBe(manifest.radius);
	});

	it('defines the same tokens in light and dark', () => {
		expect(Object.keys(manifest.dark).sort()).toEqual(Object.keys(manifest.light).sort());
	});

	it('registers every token with Tailwind', () => {
		const registered = new Set(
			[...css.matchAll(/--color-([a-z0-9-]+):\s*var\(--\1\)/g)].map(([, name]) => name)
		);
		for (const name of Object.keys(manifest.light)) {
			expect(registered.has(name), `--color-${name}`).toBe(true);
		}
	});
});

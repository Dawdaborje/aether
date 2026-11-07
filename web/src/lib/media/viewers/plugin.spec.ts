import { describe, expect, it } from 'vitest';
import { declaredMatch } from './plugin';

describe('plugin viewer matching', () => {
	const dwg = { name: 'Plan.DWG', mime: 'application/octet-stream' };
	it('matches by extension, ignoring case', () => {
		expect(declaredMatch([], ['dwg'], dwg)).toBe(true);
		expect(declaredMatch([], ['dxf'], dwg)).toBe(false);
	});
	it('matches an exact type or a whole family', () => {
		expect(declaredMatch(['model/gltf+json'], [], { name: 'a', mime: 'model/gltf+json' })).toBe(
			true
		);
		expect(declaredMatch(['model/*'], [], { name: 'a', mime: 'model/stl' })).toBe(true);
		expect(declaredMatch(['model/*'], [], { name: 'a', mime: 'image/png' })).toBe(false);
		expect(declaredMatch(['image/png'], [], { name: 'a', mime: '' })).toBe(false);
	});
});

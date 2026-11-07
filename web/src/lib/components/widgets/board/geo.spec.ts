import { describe, expect, it } from 'vitest';
import { COUNTRIES, findCountry, project } from './geo';

describe('countries', () => {
	it('are found by name, code or common alias, ignoring accents and case', () => {
		expect(findCountry('Germany')?.iso2).toBe('DE');
		expect(findCountry('de')?.name).toBe('Germany');
		expect(findCountry('DEU')?.name).toBe('Germany');
		expect(findCountry("cote d'ivoire")?.iso2).toBe('CI');
		expect(findCountry('Türkiye')?.iso2).toBe('TR');
		expect(findCountry('Turkey')?.iso2).toBe('TR');
		expect(findCountry('USA')?.name).toBe('United States');
		expect(findCountry('Atlantis')).toBeNull();
		expect(findCountry(null)).toBeNull();
		expect(findCountry('(none)')).toBeNull();
	});
	it('each have a place on the earth and none twice', () => {
		const codes = new Set(COUNTRIES.map((c) => c.iso2));
		expect(codes.size).toBe(COUNTRIES.length);
		for (const c of COUNTRIES) {
			expect(Math.abs(c.lat)).toBeLessThanOrEqual(90);
			expect(Math.abs(c.lng)).toBeLessThanOrEqual(180);
			expect(Number.isFinite(c.lat + c.lng)).toBe(true);
		}
	});
	it('are projected onto a 0 to 1 square', () => {
		const corner = project(84, -180);
		expect(corner.x).toBe(0);
		expect(corner.y).toBeCloseTo(0);
		const south = project(-58, 180);
		expect(south.x).toBe(1);
		expect(south.y).toBe(1);
	});
});

import { beforeEach, describe, expect, it } from 'vitest';
import { allViewers, registerViewer, resolveViewer, unregisterViewer } from './registry';
import type { ViewerDef } from './types';

const stub = (id: string, priority: number, ext: string): ViewerDef => ({
	id,
	label: id,
	priority,
	matches: (f) => f.name.endsWith(`.${ext}`),
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	component: null as any
});

describe('viewer registry', () => {
	beforeEach(() => {
		for (const v of allViewers()) unregisterViewer(v.id);
	});

	it('picks the highest priority viewer that matches', () => {
		registerViewer(stub('low', 10, 'dwg'));
		registerViewer(stub('high', 50, 'dwg'));
		expect(resolveViewer({ name: 'a.dwg', mime: '' })?.id).toBe('high');
		expect(resolveViewer({ name: 'a.txt', mime: '' })).toBeNull();
	});

	it('replaces a viewer registered twice and survives a broken matcher', () => {
		registerViewer(stub('a', 10, 'x'));
		registerViewer({ ...stub('a', 20, 'y') });
		expect(allViewers()).toHaveLength(1);
		registerViewer({
			...stub('bad', 99, 'z'),
			matches: () => {
				throw new Error('boom');
			}
		});
		expect(resolveViewer({ name: 'f.y', mime: '' })?.id).toBe('a');
	});
});

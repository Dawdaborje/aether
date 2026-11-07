import { describe, expect, it } from 'vitest';
import { clampWidth, narrowPane, pixels, visiblePanes } from './panes';
import type { PageNode } from '$lib/dsl/types';

const panes = (extra: Record<string, unknown> = {}): PageNode => ({
	type: 'panes',
	...extra,
	children: [
		{ type: 'pane', name: 'rail' },
		{ type: 'pane', name: 'main', grow: true },
		{ type: 'pane', name: 'thread', show: '{query.thread}' }
	]
});

describe('panes', () => {
	it('hides a pane whose condition is empty and shows it once the query has a value', () => {
		const node = panes();
		expect(visiblePanes(node, {}).map((p) => p.name)).toEqual(['rail', 'main']);
		expect(visiblePanes(node, { 'query.thread': 't1' }).map((p) => p.name)).toEqual([
			'rail',
			'main',
			'thread'
		]);
		expect(visiblePanes(node, { 'query.thread': '0' }).map((p) => p.name)).toEqual([
			'rail',
			'main'
		]);
	});

	it('can show a pane for the case when nothing is chosen', () => {
		const node: PageNode = {
			type: 'panes',
			children: [
				{ type: 'pane', name: 'welcome', show: '!{channel}' },
				{ type: 'pane', name: 'main', show: '{channel}' }
			]
		};
		expect(visiblePanes(node, {}).map((p) => p.name)).toEqual(['welcome']);
		expect(visiblePanes(node, { channel: '7' }).map((p) => p.name)).toEqual(['main']);
	});

	it('shows the detail pane on a narrow screen, or the list when nothing is chosen', () => {
		const node = panes({ focus: '{channel}' });
		const some = { channel: '7' };
		expect(narrowPane(node, visiblePanes(node, some), some)).toBe(1);
		expect(narrowPane(node, visiblePanes(node, {}), {})).toBe(0);
		const open = { channel: '7', 'query.thread': 'x' };
		expect(narrowPane(node, visiblePanes(node, open), open)).toBe(2);
		expect(narrowPane(panes(), [], {})).toBe(-1);
	});

	it('keeps a resized width inside its limits and reads pixel attributes', () => {
		expect(clampWidth(100, 200, 400)).toBe(200);
		expect(clampWidth(999, 200, 400)).toBe(400);
		expect(clampWidth(260.4, 200, 400)).toBe(260);
		expect(pixels('260')).toBe(260);
		expect(pixels('wide')).toBeUndefined();
		expect(pixels(-5)).toBeUndefined();
	});
});

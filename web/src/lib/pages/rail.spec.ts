import { describe, expect, it } from 'vitest';
import {
	badgeText,
	filterRail,
	keyOf,
	readRail,
	totalUnread,
	visibleItems,
	type RailFields
} from './rail';

const fields: RailFields = {
	id: 'id',
	label: ['name', 'label', 'title'],
	badge: 'unread',
	mention: 'mentions',
	online: 'online',
	group: 'group',
	icon: 'icon'
};

describe('rail', () => {
	it('groups a flat list by its group field, in order of first appearance', () => {
		const groups = readRail(
			[
				{ id: 'ch:1', name: 'general', group: 'Channels', unread: 3 },
				{ id: 'ch:2', name: 'secret', group: 'Private' },
				{ id: 'ch:3', name: 'random', group: 'Channels', unread: 0 },
				{ id: 'ch:4', name: 'loose' },
				{ name: 'no id' }
			],
			fields
		);
		expect(groups.map((g) => [g.label, g.items.map((i) => i.id)])).toEqual([
			['Channels', ['1', '3']],
			['Private', ['2']],
			['', ['4']]
		]);
		expect(groups[0].items[0].badge).toBe(3);
		expect(readRail([{ id: 'x', name: 'x', mentions: 2 }], fields)[0].items[0].mentions).toBe(2);
		expect(groups[0].items[1].badge).toBe(0);
	});

	it('reads an answer that already has groups, and a rows wrapper', () => {
		const nested = readRail(
			{
				groups: [
					{ id: 'team-a', label: 'Team A', items: [{ id: '1', title: 'Ops', online: true }] }
				]
			},
			fields
		);
		expect(nested[0]).toMatchObject({ key: 'team-a', label: 'Team A' });
		expect(nested[0].items[0]).toMatchObject({ id: '1', label: 'Ops', online: true });
		expect(readRail({ rows: [{ id: 'a', name: 'A' }] }, fields)[0].items).toHaveLength(1);
		expect(readRail(null, fields)).toEqual([]);
		expect(readRail('oops', fields)).toEqual([]);
	});

	it('falls back to the id for a label and ignores a bad badge', () => {
		const [g] = readRail([{ id: 'x:9', unread: 'many' }], fields);
		expect(g.items[0]).toMatchObject({ label: '9', badge: 0 });
	});

	it('filters by label and drops empty groups', () => {
		const groups = readRail(
			[
				{ id: '1', name: 'General', group: 'A' },
				{ id: '2', name: 'Ops', group: 'B' }
			],
			fields
		);
		expect(filterRail(groups, 'gen').map((g) => g.label)).toEqual(['A']);
		expect(filterRail(groups, '  ')).toBe(groups);
		expect(filterRail(groups, 'zzz')).toEqual([]);
	});

	it('writes badges, skips folded groups for the keyboard and sums unread without muted items', () => {
		expect(badgeText(7)).toBe('7');
		expect(badgeText(250)).toBe('99+');
		const groups = readRail(
			[
				{ id: '1', name: 'a', group: 'A', unread: 2 },
				{ id: '2', name: 'b', group: 'B', unread: 5, muted: true },
				{ id: '3', name: 'c', group: 'B' }
			],
			fields
		);
		expect(visibleItems(groups, new Set(['B'])).map((i) => i.id)).toEqual(['1']);
		expect(totalUnread(groups)).toBe(2);
		expect(keyOf('chat_channel:abc')).toBe('abc');
	});
});

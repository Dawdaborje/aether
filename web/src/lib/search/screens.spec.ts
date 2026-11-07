import { describe, expect, it } from 'vitest';
import type { AppTileData } from '$lib/components/apps/types';
import { buildScreenIndex, scoreScreen, searchScreens } from './screens';
import { recordHits } from './screenSearch.svelte';

const apps: AppTileData[] = [
	{
		plugin: 'tasks',
		label: 'Tasks',
		icon: 'check-square',
		route: '/tasks',
		description: 'Team tasks',
		menus: [
			{ label: 'New task', href: '/tasks/new' },
			{ label: 'Overdue tasks', href: '/tasks/overdue', group: 'Reports' }
		]
	},
	{ plugin: 'crm', label: 'Customers', icon: null, route: '/crm', description: null }
];

const index = buildScreenIndex({
	apps,
	navLinks: [{ plugin: 'party', label: 'Parties', icon: null, href: '/parties' }],
	manage: [
		{ label: 'Users', href: '/users', icon: 'users' },
		{ label: 'External', href: 'https://example.org' }
	]
});
const labels = (q: string) => searchScreens(index, q).map((entry) => entry.label);

describe('buildScreenIndex', () => {
	it('lists apps, their pages, nav links, management links and the general screens', () => {
		expect(index.map((entry) => entry.href)).toEqual([
			'/tasks',
			'/crm',
			'/tasks/new',
			'/tasks/overdue',
			'/parties',
			'/users',
			'/apps',
			'/profile'
		]);
	});

	it('names the app, and the menu group, a page belongs to', () => {
		expect(index.find((entry) => entry.href === '/tasks/overdue')?.context).toBe('Tasks · Reports');
		expect(index.find((entry) => entry.href === '/tasks/new')?.context).toBe('Tasks');
	});

	it('leaves out links that do not stay in the app, and repeats of a path', () => {
		expect(index.some((entry) => entry.href.startsWith('http'))).toBe(false);
		const again = buildScreenIndex({ apps: [...apps, ...apps] });
		expect(again.filter((entry) => entry.href === '/tasks')).toHaveLength(1);
	});
});

describe('searchScreens', () => {
	it('lists the first screens for an empty query', () => {
		expect(searchScreens(index, '  ', 3)).toHaveLength(3);
	});

	it('ranks an exact label above a prefix and a prefix above the rest', () => {
		expect(labels('tasks')[0]).toBe('Tasks');
		expect(labels('over')[0]).toBe('Overdue tasks');
	});

	it('matches the start of any word of the label', () => {
		expect(labels('task')).toContain('New task');
		expect(labels('rep')).toContain('Overdue tasks'); // the section "Reports"
	});

	it('needs every word of the query', () => {
		expect(labels('new task')).toEqual(['New task']);
		expect(labels('new zebra')).toEqual([]);
	});

	it('ignores case and accents', () => {
		expect(labels('CUSTOMERS')).toEqual(['Customers']);
		expect(
			scoreScreen({ id: 'x', label: 'Café', href: '/c', kind: 'page' }, 'cafe')
		).toBeGreaterThan(0);
	});

	it('finds an app by its description', () => {
		expect(labels('team')).toContain('Tasks');
	});

	it('forgives a missing letter in a word of three or more letters', () => {
		expect(labels('cstmrs')).toContain('Customers');
		expect(labels('zz')).toEqual([]);
	});

	it('puts apps before pages when scores tie', () => {
		const tie = buildScreenIndex({
			apps: [
				{ plugin: 'a', label: 'Reports', icon: null, route: '/a', description: null, menus: [] }
			],
			manage: [{ label: 'Reports', href: '/reports' }]
		});
		expect(searchScreens(tie, 'reports').map((entry) => entry.kind)).toEqual(['app', 'manage']);
	});
});

describe('palette entries', () => {
	const withPalette = buildScreenIndex({
		apps: [],
		palette: [
			{
				plugin: 'leave',
				app: 'Leave',
				label: 'New leave request',
				href: '/leave/new',
				function: null,
				search: null,
				icon: 'plus',
				keywords: 'holiday absence',
				shortcut: 'alt+l'
			},
			{
				plugin: 'leave',
				app: 'Leave',
				label: 'Outside',
				href: 'https://example.com',
				function: null,
				search: null,
				icon: null,
				keywords: null,
				shortcut: null
			}
		]
	});

	it('lists app paths first, as commands with their shortcut', () => {
		const command = withPalette.find((entry) => entry.kind === 'command');
		expect(withPalette[0]).toBe(command);
		expect(command).toMatchObject({ href: '/leave/new', context: 'Leave', shortcut: 'alt+l' });
		expect(withPalette.some((entry) => entry.href.startsWith('http'))).toBe(false);
	});

	it('is found by its keywords', () => {
		expect(searchScreens(withPalette, 'holiday')[0]?.label).toBe('New leave request');
	});
});

describe('function and search entries', () => {
	const index = buildScreenIndex({
		apps: [],
		palette: [
			{
				plugin: 'leave',
				app: 'Leave',
				label: 'Start a request',
				href: null,
				function: 'leave.start_request',
				search: null,
				icon: null,
				keywords: null,
				shortcut: 'alt+r'
			},
			{
				plugin: 'leave',
				app: 'Leave',
				label: 'Requests',
				href: null,
				function: null,
				search: 'leave.find_requests',
				icon: null,
				keywords: null,
				shortcut: null
			}
		]
	});

	it('lists a function entry that runs, and leaves a search provider out of the list', () => {
		const commands = index.filter((entry) => entry.kind === 'command');
		expect(commands).toHaveLength(1);
		expect(commands[0]).toMatchObject({ run: 'leave.start_request', href: '', shortcut: 'alt+r' });
	});
});

describe('record hits', () => {
	it('keeps labelled items that open app paths, with the app as context', () => {
		const hits = recordHits('Leave', [
			{ label: 'Ada Lovelace', href: '/leave/7', context: 'Annual' },
			{ label: ' ', href: '/leave/8' },
			{ label: 'Elsewhere', href: 'https://example.com' },
			{ label: 'Sneaky', href: '//example.com' },
			{ label: 'Plain', href: '/leave/9' },
			null
		]);
		expect(hits.map((hit) => hit.label)).toEqual(['Ada Lovelace', 'Plain']);
		expect(hits[0]?.context).toBe('Leave · Annual');
		expect(hits[1]?.context).toBe('Leave');
	});

	it('reads a { rows } answer and caps the list', () => {
		const rows = Array.from({ length: 9 }, (_, i) => ({ label: `r${i}`, href: `/r/${i}` }));
		expect(recordHits('A', { rows })).toHaveLength(5);
		expect(recordHits('A', 'nope')).toEqual([]);
	});
});

import { describe, expect, it } from 'vitest';
import { buildPage } from './buildPage';

describe('buildPage kanban templates', () => {
	it('flattens templates/card and templates/menu into card + actions', () => {
		const { page } = buildPage({
			type: 'page',
			title: 'Board',
			children: [
				{
					type: 'view',
					viewType: 'kanban',
					source: 'list_tickets',
					group_by: 'stage',
					children: [
						{
							type: 'templates',
							children: [
								{
									type: 'card',
									children: [{ type: 'field', name: 'title' }]
								},
								{
									type: 'menu',
									children: [{ type: 'action', name: 'archive', function: 'archive_ticket' }]
								}
							]
						},
						{ type: 'progressbar', field: 'priority' }
					]
				}
			]
		});

		const view = page.children?.[0];
		expect(view?.type).toBe('view');
		const types = (view?.children ?? []).map((c) => c.type);
		expect(types).toContain('card');
		expect(types).toContain('actions');
		expect(types).toContain('progressbar');
		expect(types).not.toContain('templates');
		expect(types).not.toContain('menu');
		const actions = view?.children?.find((c) => c.type === 'actions');
		expect(actions?.children?.[0]?.function).toBe('archive_ticket');
	});
});

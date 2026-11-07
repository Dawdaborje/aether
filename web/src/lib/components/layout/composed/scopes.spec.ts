import { describe, expect, it } from 'vitest';
import { parseScopes } from './scopes';

describe('scope switcher answers', () => {
	it('reads a list with a marked current item', () => {
		const scopes = parseScopes([
			{ id: 1, label: 'Alpha', href: '/p/1' },
			{ id: 2, label: ' Beta ', href: '/p/2', current: true }
		]);
		expect(scopes.items.map((i) => i.label)).toEqual(['Alpha', 'Beta']);
		expect(scopes.current?.id).toBe('2');
	});

	it('reads { current, items } and drops unsafe or unlabelled items', () => {
		const scopes = parseScopes({
			current: 'b',
			items: [
				{ id: 'a', label: 'A', href: '/a' },
				{ id: 'b', label: 'B', href: 'https://example.com/b' },
				{ id: 'c', label: 'C', href: 'javascript:alert(1)' },
				{ id: 'd', label: '', href: '/d' },
				{ id: 'e', label: 'E', href: '//evil.example' }
			]
		});
		expect(scopes.items.map((i) => i.id)).toEqual(['a', 'b']);
		expect(scopes.current?.label).toBe('B');
	});

	it('is empty for anything else', () => {
		expect(parseScopes(null)).toEqual({ current: null, items: [] });
		expect(parseScopes('x')).toEqual({ current: null, items: [] });
	});
});

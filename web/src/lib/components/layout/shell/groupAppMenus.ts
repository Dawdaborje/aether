import type { NavItem } from '$lib/theme/types';

type AppMenu = {
	label: string;
	href: string;
	icon?: string | null;
	group?: string | null;
};

/**
 * Flat app menus → sidebar NavItems.
 *
 * - No `group` → top-level link.
 * - A group with 2+ pages → collapsible section (Frappe-style).
 * - A group with only one page → that page as a top-level link (no redundant
 *   "ATTENDANCE ▾ / Attendance" dropdown).
 *
 * Order follows first appearance in `items`.
 */
export function groupAppMenus(items: AppMenu[]): NavItem[] {
	type Bucket =
		| { kind: 'flat'; item: NavItem }
		| { kind: 'group'; label: string; children: NavItem[] };

	const order: string[] = [];
	const buckets = new Map<string, Bucket>();

	for (const item of items) {
		const entry: NavItem = {
			label: item.label,
			href: item.href,
			icon: item.icon ?? undefined
		};
		const group = item.group?.trim();
		if (!group) {
			const key = `flat:${item.href}:${item.label}`;
			if (!buckets.has(key)) {
				order.push(key);
				buckets.set(key, { kind: 'flat', item: entry });
			}
			continue;
		}
		const key = `group:${group}`;
		const existing = buckets.get(key);
		if (existing?.kind === 'group') {
			existing.children.push(entry);
		} else {
			order.push(key);
			buckets.set(key, { kind: 'group', label: group, children: [entry] });
		}
	}

	const result: NavItem[] = [];
	for (const key of order) {
		const bucket = buckets.get(key);
		if (!bucket) continue;
		if (bucket.kind === 'flat') {
			result.push(bucket.item);
			continue;
		}
		if (bucket.children.length === 1 && bucket.children[0]) {
			// One page in the section: show the link, not a dropdown of one.
			result.push(bucket.children[0]);
		} else {
			result.push({ label: bucket.label, children: bucket.children });
		}
	}
	return result;
}

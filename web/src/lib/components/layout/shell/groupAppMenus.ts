import type { NavItem } from '$lib/theme/types';

type AppMenu = {
	label: string;
	href: string;
	icon?: string | null;
	group?: string | null;
	/** Lucide icon for the collapsible section (`menu_group_icon` on the page). */
	group_icon?: string | null;
};

/**
 * Flat app menus → sidebar NavItems.
 *
 * - No `group` → top-level link.
 * - A group with 2+ pages → collapsible section.
 * - A group with only one page → that page as a top-level link (no redundant
 *   "ATTENDANCE ▾ / Attendance" dropdown).
 *
 * Ungrouped links (e.g. Home) come first, then groups in first-appearance order.
 */
export function groupAppMenus(items: AppMenu[]): NavItem[] {
	type Bucket =
		| { kind: 'flat'; item: NavItem }
		| { kind: 'group'; label: string; icon?: string; children: NavItem[] };

	const flatOrder: string[] = [];
	const groupOrder: string[] = [];
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
				flatOrder.push(key);
				buckets.set(key, { kind: 'flat', item: entry });
			}
			continue;
		}
		const key = `group:${group}`;
		const groupIcon = item.group_icon?.trim() || undefined;
		const existing = buckets.get(key);
		if (existing?.kind === 'group') {
			existing.children.push(entry);
			if (!existing.icon && groupIcon) existing.icon = groupIcon;
		} else {
			groupOrder.push(key);
			buckets.set(key, {
				kind: 'group',
				label: group,
				icon: groupIcon,
				children: [entry]
			});
		}
	}

	const result: NavItem[] = [];
	for (const key of [...flatOrder, ...groupOrder]) {
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
			const icon =
				bucket.icon ??
				bucket.children.find((child) => child.icon)?.icon ??
				'folder';
			result.push({ label: bucket.label, icon, children: bucket.children });
		}
	}
	return result;
}

import type { NavItem } from '$lib/theme/types';
import { isActive } from '../../nav';

/** How a `nav` or `app-menu` block draws its items (see "Navigation" in docs/architecture/themes.md). */
export const NAV_VARIANTS = ['links', 'tree', 'tabs', 'mega', 'columns', 'tiles'] as const;
export type NavVariant = (typeof NAV_VARIANTS)[number];

/** Which groups of a `tree` are open: the user's choice, one at a time, or all of them. */
export const NAV_EXPANDS = ['toggle', 'active', 'all'] as const;
export type NavExpand = (typeof NAV_EXPANDS)[number];

export function pick<T extends string>(choices: readonly T[], value: string, fallback: T): T {
	return (choices as readonly string[]).includes(value) ? (value as T) : fallback;
}

/** A stable key for an item among its siblings. */
export const itemKey = (item: NavItem) => item.label + '\u0000' + (item.href ?? '');

/** Every link in the tree, depth first. */
export function allLinks(items: NavItem[]): NavItem[] {
	return items.flatMap((item) => [...(item.href ? [item] : []), ...allLinks(item.children ?? [])]);
}

/**
 * The one link that is the current page: the longest href that covers `pathname`, so
 * "Employees" is not current beside "Employees › New". `null` when none matches.
 */
export function currentLink(items: NavItem[], pathname: string): NavItem | null {
	let best: NavItem | null = null;
	for (const link of allLinks(items)) {
		if (!isActive(link, pathname)) continue;
		if (!best || (link.href?.length ?? 0) > (best.href?.length ?? 0)) best = link;
	}
	return best;
}

/** True when `item` is `current` or holds it somewhere below. */
export function holds(item: NavItem, current: NavItem | null): boolean {
	if (!current) return false;
	return item === current || (item.children ?? []).some((child) => holds(child, current));
}

/**
 * How many tabs fit in `available` pixels, leaving room for a "More" button of `more` pixels
 * when some do not. `widths` are the tabs' own widths, `gap` the space between them.
 */
export function fitCount(widths: number[], available: number, more: number, gap = 0): number {
	const total = widths.reduce((sum, w) => sum + w, 0) + gap * Math.max(0, widths.length - 1);
	if (total <= available) return widths.length;
	let used = more;
	for (let i = 0; i < widths.length; i++) {
		used += widths[i]! + gap;
		if (used > available) return i;
	}
	return widths.length;
}

/**
 * The columns of a mega panel. Children that have children of their own become columns
 * headed by their label; plain links are gathered into one column without a heading, first.
 * Without any nested children every link is its own cell (`columns` is empty).
 */
export function megaColumns(group: NavItem): {
	columns: Array<{ heading: string | null; links: NavItem[] }>;
	cells: NavItem[];
} {
	const children = group.children ?? [];
	if (!children.some((child) => child.children?.length)) return { columns: [], cells: children };
	const loose = children.filter((child) => !child.children?.length);
	const columns = children
		.filter((child) => child.children?.length)
		.map((child) => ({
			heading: child.label,
			links: [
				...(child.href ? [{ ...child, children: undefined }] : []),
				...allLinks(child.children!)
			]
		}));
	return {
		columns: loose.length ? [{ heading: null, links: loose }, ...columns] : columns,
		cells: []
	};
}

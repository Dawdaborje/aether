/**
 * The logic of the `<rail>` navigation list, apart from the drawing: reading what a source function answers,
 * grouping, filtering and the text of a badge.
 */
export type Row = Record<string, unknown>;

export interface RailFields {
	id: string;
	label: string[];
	badge: string;
	mention: string;
	online: string;
	group: string;
	icon: string;
}

export interface RailItem {
	id: string;
	label: string;
	badge: number;
	/** Unread messages that mention the person: drawn louder than the plain count. */
	mentions: number;
	online: boolean;
	muted: boolean;
	icon: string;
	row: Row;
}

export interface RailGroup {
	/** Stable key of the group (used to remember it folded). */
	key: string;
	/** Heading text; empty for items that belong to no group. */
	label: string;
	items: RailItem[];
}

/** `table:abc` -> `abc`. */
export function keyOf(value: unknown): string {
	const text = String(value ?? '');
	const colon = text.indexOf(':');
	return colon >= 0 ? text.slice(colon + 1) : text;
}

function toItem(row: Row, fields: RailFields): RailItem | null {
	const id = keyOf(row[fields.id]);
	if (!id) return null;
	const label = fields.label
		.map((f) => row[f])
		.find((v) => typeof v === 'string' && v.trim() !== '');
	const badge = Number(row[fields.badge]);
	const mentions = Number(row[fields.mention]);
	return {
		id,
		label: typeof label === 'string' ? label : id,
		badge: Number.isFinite(badge) && badge > 0 ? Math.floor(badge) : 0,
		mentions: Number.isFinite(mentions) && mentions > 0 ? Math.floor(mentions) : 0,
		online: row[fields.online] === true,
		muted: row.muted === true,
		icon: typeof row[fields.icon] === 'string' ? (row[fields.icon] as string) : '',
		row
	};
}

/**
 * What a source answers: a flat list of rows (grouped by their `group` field when they have one), or
 * `{ groups: [{ id?, label, items: [...] }] }`. Rows without an id are left out.
 */
export function readRail(data: unknown, fields: RailFields): RailGroup[] {
	const groups: RailGroup[] = [];
	const nested =
		data && typeof data === 'object' && !Array.isArray(data)
			? (data as { groups?: unknown }).groups
			: undefined;
	if (Array.isArray(nested)) {
		for (const [i, g] of nested.entries()) {
			if (!g || typeof g !== 'object') continue;
			const group = g as {
				id?: unknown;
				key?: unknown;
				label?: unknown;
				name?: unknown;
				items?: unknown;
			};
			const items = (Array.isArray(group.items) ? (group.items as Row[]) : [])
				.map((row) => toItem(row, fields))
				.filter((item): item is RailItem => item !== null);
			const label = String(group.label ?? group.name ?? '');
			groups.push({ key: String(group.key ?? group.id ?? label ?? i), label, items });
		}
		return groups;
	}
	const rows = Array.isArray(data)
		? (data as Row[])
		: data && typeof data === 'object' && Array.isArray((data as { rows?: unknown }).rows)
			? (data as { rows: Row[] }).rows
			: [];
	const byKey = new Map<string, RailGroup>();
	for (const row of rows) {
		const item = toItem(row, fields);
		if (!item) continue;
		const label = typeof row[fields.group] === 'string' ? (row[fields.group] as string) : '';
		let group = byKey.get(label);
		if (!group) {
			group = { key: label, label, items: [] };
			byKey.set(label, group);
			groups.push(group);
		}
		group.items.push(item);
	}
	return groups;
}

/** Keep the items whose label contains `needle` (case-insensitive); groups left empty disappear. */
export function filterRail(groups: RailGroup[], needle: string): RailGroup[] {
	const n = needle.trim().toLowerCase();
	if (!n) return groups;
	return groups
		.map((g) => ({ ...g, items: g.items.filter((item) => item.label.toLowerCase().includes(n)) }))
		.filter((g) => g.items.length > 0);
}

/** The text in an unread badge: "7", "99+". */
export function badgeText(count: number): string {
	return count > 99 ? '99+' : String(count);
}

/** The items in reading order, skipping folded groups: what the arrow keys move through. */
export function visibleItems(groups: RailGroup[], folded: Set<string>): RailItem[] {
	return groups.flatMap((g) => (folded.has(g.key) ? [] : g.items));
}

/** Total unread across the list, for a heading or the page title. */
export function totalUnread(groups: RailGroup[]): number {
	return groups.reduce(
		(sum, g) => sum + g.items.reduce((n, item) => n + (item.muted ? 0 : item.badge), 0),
		0
	);
}

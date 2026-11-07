/**
 * The logic of the `<messages>` list, apart from the drawing: merging rows that arrive live, grouping them by
 * author and day, and keeping the list a bounded size.
 */
export type Row = Record<string, unknown>;

export interface Fields {
	id: string;
	author: string;
	/** A field holding the author's display name, when the row has one. */
	name: string;
	/** A field holding the address of the author's profile picture, when the row has one. */
	image: string;
	time: string;
	body: string;
	/** A field holding attached files (`[{ key, name, mime, size }]`). */
	files: string;
}

/** The key of a row as text (`chat_message:abc` -> `abc`), or `''` when it has none. */
export function rowKey(row: Row, field = 'id'): string {
	const raw = String(row[field] ?? '');
	const colon = raw.indexOf(':');
	return colon >= 0 ? raw.slice(colon + 1) : raw;
}

/**
 * Put `incoming` rows into `rows`: one with a key already there replaces it in place (an edit), the others are
 * appended in the order they came. Rows without a key are always appended.
 */
export function upsertRows(rows: Row[], incoming: Row[], field = 'id'): Row[] {
	const next = rows.slice();
	const at = new Map<string, number>();
	next.forEach((row, i) => {
		const key = rowKey(row, field);
		if (key) at.set(key, i);
	});
	for (const row of incoming) {
		const key = rowKey(row, field);
		const found = key ? at.get(key) : undefined;
		if (found !== undefined) next[found] = row;
		else {
			if (key) at.set(key, next.length);
			next.push(row);
		}
	}
	return next;
}

/** Put older rows in front, leaving out any already held. */
export function prependRows(rows: Row[], older: Row[], field = 'id'): Row[] {
	const have = new Set(rows.map((r) => rowKey(r, field)).filter(Boolean));
	return [...older.filter((r) => !have.has(rowKey(r, field)) || !rowKey(r, field)), ...rows];
}

export function removeRow(rows: Row[], id: string, field = 'id'): Row[] {
	return rows.filter((row) => rowKey(row, field) !== id);
}

/** Keep at most `max` rows from one end, so a very long chat does not grow without limit. */
export function trimRows(rows: Row[], max: number, keep: 'newest' | 'oldest'): Row[] {
	if (rows.length <= max) return rows;
	return keep === 'newest' ? rows.slice(rows.length - max) : rows.slice(0, max);
}

export interface Item {
	row: Row;
	key: string;
	/** A day heading goes above this row. */
	day: string | null;
	/** This row starts a run by a new author (or after a pause): show who wrote it. */
	head: boolean;
}

const RUN_MS = 5 * 60 * 1000;

function timeOf(row: Row, field: string): number {
	const t = Date.parse(String(row[field] ?? ''));
	return Number.isFinite(t) ? t : NaN;
}

function authorOf(row: Row, field: string): string {
	const value = row[field];
	if (value && typeof value === 'object') {
		const o = value as Record<string, unknown>;
		return String(o.display_name ?? o.name ?? o.id ?? '');
	}
	return String(value ?? '');
}

/**
 * The name to show for a row's author: the display-name field when the row has one, otherwise the author field
 * (a name, or an object with `display_name`/`name`), with a leading `table:` of a record id removed.
 */
export function displayName(row: Row, fields: Pick<Fields, 'author' | 'name'>): string {
	const named = row[fields.name];
	const raw =
		typeof named === 'string' && named.trim() !== '' ? named : authorOf(row, fields.author);
	return raw.replace(/^[a-z_]+:/, '');
}

/** Day headings and author runs, for drawing. `dayLabel` turns a time into the heading text. */
export function groupRows(rows: Row[], fields: Fields, dayLabel: (time: number) => string): Item[] {
	const items: Item[] = [];
	let lastDay = '';
	let previous: Row | null = null;
	for (const row of rows) {
		const t = timeOf(row, fields.time);
		const day = Number.isNaN(t) ? '' : dayLabel(t);
		const newDay = day !== '' && day !== lastDay;
		if (day) lastDay = day;
		const sameRun =
			previous !== null &&
			!newDay &&
			authorOf(previous, fields.author) === authorOf(row, fields.author) &&
			(Number.isNaN(t) ||
				Number.isNaN(timeOf(previous, fields.time)) ||
				t - timeOf(previous, fields.time) < RUN_MS);
		items.push({ row, key: rowKey(row, fields.id), day: newDay ? day : null, head: !sameRun });
		previous = row;
	}
	return items;
}

/** The people typing as a sentence: "Ann is typing", "Ann and Bob are typing", "Several people are typing". */
export function typingText(names: string[]): string {
	if (names.length === 0) return '';
	if (names.length === 1) return `${names[0]} is typing…`;
	if (names.length === 2) return `${names[0]} and ${names[1]} are typing…`;
	return 'Several people are typing…';
}

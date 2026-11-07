/** Helpers for showing who is connected: ids, names, initials and how many avatars fit. */
export interface Person {
	id: string;
	name: string;
	image?: string;
}

/** The key of a user id (`users:abc` -> `abc`), so ids from different places compare equal. */
export function userKey(id: unknown): string {
	const text = String(id ?? '');
	const colon = text.indexOf(':');
	return colon >= 0 ? text.slice(colon + 1) : text;
}

export function initial(name: string): string {
	return name.trim().charAt(0).toUpperCase() || '?';
}

/**
 * The people to draw for the connected `users`: names come from what the plugin told us (`known`, by key), and a
 * person nobody named is shown by the end of their id.
 */
export function peopleFor(users: string[], known: Map<string, Person>): Person[] {
	const seen = new Set<string>();
	const out: Person[] = [];
	for (const user of users) {
		const key = userKey(user);
		if (!key || seen.has(key)) continue;
		seen.add(key);
		out.push(known.get(key) ?? { id: key, name: key.slice(-6) });
	}
	return out.sort((a, b) => a.name.localeCompare(b.name));
}

/** The first `max` people, and how many more there are. */
export function fit(people: Person[], max: number): { shown: Person[]; more: number } {
	const limit = Math.max(1, max);
	return { shown: people.slice(0, limit), more: Math.max(0, people.length - limit) };
}

/** What a names function answers (`[{ id, name }]`, or `{ people: [...] }`), as a map by key. */
export function readPeople(data: unknown): Map<string, Person> {
	const rows = Array.isArray(data)
		? data
		: data && typeof data === 'object' && Array.isArray((data as { people?: unknown }).people)
			? (data as { people: unknown[] }).people
			: [];
	const map = new Map<string, Person>();
	for (const row of rows) {
		if (!row || typeof row !== 'object') continue;
		const r = row as Record<string, unknown>;
		const id = userKey(r.id);
		if (!id) continue;
		const name = String(r.name ?? r.display_name ?? '').trim() || id.slice(-6);
		map.set(id, { id, name, image: typeof r.image === 'string' ? r.image : undefined });
	}
	return map;
}

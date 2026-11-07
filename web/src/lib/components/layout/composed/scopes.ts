export interface Scope {
	id: string;
	label: string;
	/** An app path or an http(s) URL. */
	href: string;
}

export interface Scopes {
	current: Scope | null;
	items: Scope[];
}

const safeHref = (href: string) =>
	(href.startsWith('/') && !href.startsWith('//')) || /^https?:\/\//.test(href);

/**
 * What a `scope-switcher` function answered: a list of `{ id, label, href, current? }`, or
 * `{ current: "<id>", items: [...] }`. Items without a label or with an unsafe `href` are dropped.
 */
export function parseScopes(answer: unknown): Scopes {
	const object = answer && typeof answer === 'object' && !Array.isArray(answer);
	const list = Array.isArray(answer)
		? answer
		: object && Array.isArray((answer as { items?: unknown }).items)
			? (answer as { items: unknown[] }).items
			: [];
	let currentId =
		object && typeof (answer as { current?: unknown }).current === 'string'
			? (answer as { current: string }).current
			: null;
	const items: Scope[] = [];
	for (const entry of list.slice(0, 50)) {
		if (!entry || typeof entry !== 'object') continue;
		const { id, label, href, current } = entry as Record<string, unknown>;
		if (typeof label !== 'string' || !label.trim()) continue;
		if (typeof href !== 'string' || !safeHref(href)) continue;
		const key = typeof id === 'string' || typeof id === 'number' ? String(id) : href;
		items.push({ id: key, label: label.trim().slice(0, 80), href });
		if (current === true && currentId === null) currentId = key;
	}
	return { current: items.find((item) => item.id === currentId) ?? null, items };
}

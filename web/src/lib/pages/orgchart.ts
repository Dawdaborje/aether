/** The logic of the `<orgchart>` widget, apart from the drawing. */
export interface ChartNode {
	id: string;
	name: string;
	title: string;
	photo: unknown;
	children: ChartNode[];
}

const keyOf = (value: unknown): string => {
	const text = String(value ?? '');
	const colon = text.indexOf(':');
	return colon >= 0 ? text.slice(colon + 1) : text;
};

/**
 * Rows of `{ id, name, title?, manager?, photo? }` as a forest: a row whose manager is not among the rows (or is
 * itself, or would make a loop) starts a tree. Children keep the order of the rows.
 */
export function orgTree(data: unknown): ChartNode[] {
	const rows = (Array.isArray(data) ? data : (data as { rows?: unknown[] } | null)?.rows ?? []) as Record<
		string,
		unknown
	>[];
	const nodes = new Map<string, ChartNode>();
	const parents = new Map<string, string>();
	for (const row of rows) {
		const id = keyOf(row.id);
		if (!id || nodes.has(id)) continue;
		nodes.set(id, {
			id,
			name: String(row.name ?? id),
			title: String(row.title ?? ''),
			photo: row.photo ?? null,
			children: []
		});
		parents.set(id, keyOf(row.manager));
	}
	const reaches = (from: string, target: string): boolean => {
		for (let i = 0, at = from; i < 100 && at; i++, at = parents.get(at) ?? '') {
			if (at === target) return true;
		}
		return false;
	};
	const roots: ChartNode[] = [];
	for (const [id, node] of nodes) {
		const parent = parents.get(id) ?? '';
		if (parent && parent !== id && nodes.has(parent) && !reaches(parent, id)) {
			nodes.get(parent)!.children.push(node);
		} else roots.push(node);
	}
	return roots;
}

/** Up to two capital letters from a name. */
export function initials(name: string): string {
	return name
		.split(/\s+/)
		.filter(Boolean)
		.slice(0, 2)
		.map((part) => part[0]!.toUpperCase())
		.join('');
}

import { apiFetch } from '$lib/api/client';

/** What a record is called: its model's `title_field`, looked up by the kernel. */
const known = new Map<string, string>();
const missing = new Set<string>();

const keyOf = (plugin: string, target: string, id: string) => `${plugin}|${target}|${id}`;

/**
 * The titles of `ids` of the model a link field points at (`target`, `department` or
 * `hr.department`, read from the page of `plugin`). Ids that have no title are left out, so
 * callers keep showing the id. Answers are remembered for the session.
 */
export async function recordTitles(
	plugin: string,
	target: string,
	ids: string[]
): Promise<Record<string, string>> {
	const wanted = [...new Set(ids.filter((id) => id.includes(':')))];
	const out: Record<string, string> = {};
	const toAsk: string[] = [];
	for (const id of wanted) {
		const key = keyOf(plugin, target, id);
		const have = known.get(key);
		if (have !== undefined) out[id] = have;
		else if (!missing.has(key)) toAsk.push(id);
	}
	if (toAsk.length > 0) {
		try {
			const res = await apiFetch('/api/ui/records/titles', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ plugin, target, ids: toAsk })
			});
			if (res.ok) {
				const body = (await res.json()) as { titles?: Record<string, string> };
				const found = body.titles ?? {};
				for (const id of toAsk) {
					const title = found[id] ?? found[id.replace(/[⟨⟩]/g, '')];
					if (title) {
						known.set(keyOf(plugin, target, id), title);
						out[id] = title;
					} else {
						missing.add(keyOf(plugin, target, id));
					}
				}
			}
		} catch {
			// Offline or refused: the ids stay as they are.
		}
	}
	return out;
}

/** One record of a model, as a link picker lists it. */
export interface RecordOption {
	id: string;
	title: string;
}

/**
 * Records of the model a link points at, matching `q` by title, for a picker that has no
 * `source` function of its own. Titles are remembered too, so rows showing these ids by name
 * need no second request.
 */
export async function recordOptions(
	plugin: string,
	target: string,
	q = '',
	limit = 50
): Promise<RecordOption[]> {
	try {
		const res = await apiFetch('/api/ui/records/options', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ plugin, target, q, limit })
		});
		if (!res.ok) return [];
		const body = (await res.json()) as { options?: RecordOption[] };
		const options = body.options ?? [];
		for (const option of options) known.set(keyOf(plugin, target, option.id), option.title);
		return options;
	} catch {
		return [];
	}
}

/**
 * For rows being shown: the title of every linked record in `fields` (a field name and the model
 * it points at), as `{ field: { id: title } }`.
 */
export async function titlesForRows(
	plugin: string,
	rows: Record<string, unknown>[],
	fields: Map<string, string>
): Promise<Record<string, Record<string, string>>> {
	const out: Record<string, Record<string, string>> = {};
	await Promise.all(
		[...fields].map(async ([field, target]) => {
			const ids = rows
				.map((row) => row[field])
				.filter((value): value is string => typeof value === 'string' && value !== '');
			if (ids.length === 0) return;
			const found = await recordTitles(plugin, target, ids);
			if (Object.keys(found).length > 0) out[field] = found;
		})
	);
	return out;
}

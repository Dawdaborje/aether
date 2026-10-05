import { getContext, setContext } from 'svelte';
import { apiFetch } from '$lib/api/client';

const PAGE = Symbol('aether.page');
const FORM = Symbol('aether.form');

/** What calling a plugin function came back with. */
export interface CallResult {
	ok: boolean;
	status: number;
	data: unknown;
	/** The message to show when it failed: the plugin's own when it wrote one. */
	error: string | null;
}

/**
 * The page being shown: which plugin it belongs to, the values its route captured, and a way
 * to call that plugin's functions. Widgets read it so a page can be written as XML that names
 * functions (`source="list_notes"`, `function="add_note"`) without any script.
 */
export class PageState {
	plugin = $state('');
	/** The model the page is about (`<page model="note">`), when it names one. */
	model = $state('');
	params = $state<Record<string, string>>({});
	/** Goes up whenever something changed data, so lists on the page load again. */
	tick = $state(0);

	refresh(): void {
		this.tick += 1;
	}

	/** Call one of this plugin's functions as the person looking at the page. */
	async call(fn: string, payload: Record<string, unknown> = {}): Promise<CallResult> {
		return callPlugin(this.plugin, fn, payload);
	}
}

export function setPageState(state: PageState): void {
	setContext(PAGE, state);
}

export function getPageState(): PageState | undefined {
	return getContext<PageState | undefined>(PAGE);
}

/** The values of a form's fields, shared with the fields inside it. */
export class FormState {
	values = $state<Record<string, unknown>>({});
}

export function setFormState(state: FormState): void {
	setContext(FORM, state);
}

export function getFormState(): FormState | undefined {
	return getContext<FormState | undefined>(FORM);
}

/** `POST /api/plugins/<plugin>/<function>`. Never throws: a failure is in the result. */
export async function callPlugin(
	plugin: string,
	fn: string,
	payload: Record<string, unknown> = {}
): Promise<CallResult> {
	if (!plugin) return { ok: false, status: 0, data: null, error: 'This page has no plugin to call.' };
	try {
		const res = await apiFetch(`/api/plugins/${encodeURIComponent(plugin)}/${encodeURIComponent(fn)}`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(payload)
		});
		const text = await res.text();
		let data: unknown = null;
		try {
			data = text ? JSON.parse(text) : null;
		} catch {
			data = text;
		}
		if (res.ok) return { ok: true, status: res.status, data, error: null };
		const message =
			data && typeof data === 'object' && 'error' in data && typeof data.error === 'string'
				? data.error
				: `The request failed (${res.status}).`;
		return { ok: false, status: res.status, data, error: message };
	} catch {
		return { ok: false, status: 0, data: null, error: 'The server could not be reached.' };
	}
}

/** The rows of a function's answer: a list, or an object holding one under `rows`/`items`/`data`. */
export function rowsOf(data: unknown): Record<string, unknown>[] {
	const list = Array.isArray(data)
		? data
		: data && typeof data === 'object'
			? (['rows', 'items', 'data'] as const)
					.map((key) => (data as Record<string, unknown>)[key])
					.find(Array.isArray)
			: undefined;
	return (list ?? []).filter(
		(row): row is Record<string, unknown> => row !== null && typeof row === 'object'
	);
}

/** `notes_note:abc` is the record `abc` of its table; routes and functions take the key. */
export function keyOf(id: unknown): string {
	const text = String(id ?? '');
	const colon = text.indexOf(':');
	return colon >= 0 ? text.slice(colon + 1) : text;
}

/** Fill `{field}` placeholders of a route from a row (`/notes/{id}`); `id` becomes the key. */
export function interpolate(template: string, row: Record<string, unknown>): string {
	return template.replace(/\{([a-z0-9_]+)\}/g, (_, field: string) =>
		encodeURIComponent(field === 'id' ? keyOf(row[field]) : String(row[field] ?? ''))
	);
}

/** A node attribute as text, or `undefined` when it is missing or empty. */
export function text(value: unknown): string | undefined {
	return typeof value === 'string' && value.trim() !== '' ? value : undefined;
}

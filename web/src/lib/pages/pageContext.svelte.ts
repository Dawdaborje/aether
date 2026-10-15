import { getContext, setContext } from 'svelte';
import { apiFetch } from '$lib/api/client';

const PAGE = Symbol('aether.page');
const FORM = Symbol('aether.form');

/** What kind of failure a call was: drives the title of the error modal. */
export type ErrorKind =
	'validation' | 'server' | 'network' | 'permission' | 'not_found' | 'rate_limit' | 'client' | 'plugin_load';

/** What calling a plugin function came back with. */
export interface CallResult {
	ok: boolean;
	status: number;
	data: unknown;
	/** The message to show when it failed: the plugin's own when it wrote one. */
	error: string | null;
	/** Set when it failed. */
	kind: ErrorKind | null;
}

export interface PageError {
	kind: ErrorKind;
	status: number;
	message: string;
	plugin: string;
	fn: string;
	/** What went wrong inside the plugin; the server only sends it to developers. */
	detail?: string;
}

/** The failure the error modal is showing, if any. */
export const pageError = $state<{ current: PageError | null }>({ current: null });

export function classifyStatus(status: number): ErrorKind {
	if (status === 0) return 'network';
	if (status === 400 || status === 409 || status === 422) return 'validation';
	if (status === 401 || status === 403) return 'permission';
	if (status === 404) return 'not_found';
	if (status === 429) return 'rate_limit';
	if (status >= 500) return 'server';
	return 'client';
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

	/**
	 * Call one of this plugin's functions as the person looking at the page. `crm.search_stages`
	 * names a function of another plugin (a picker for a record that plugin owns); the server
	 * still checks the person may call it, exactly as for a call made any other way.
	 */
	async call(fn: string, payload: Record<string, unknown> = {}): Promise<CallResult> {
		const dot = fn.indexOf('.');
		if (dot > 0) return callPlugin(fn.slice(0, dot), fn.slice(dot + 1), payload);
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
	/** Saves the form (what its Save button does); set by the form view. */
	save = $state<(() => Promise<boolean>) | null>(null);
	saving = $state(false);
	/** The form's own Save label (`submit="…"`). */
	saveLabel = $state('Save');
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
	if (!plugin)
		return {
			ok: false,
			status: 0,
			data: null,
			error: 'This page has no plugin to call.',
			kind: 'client'
		};
	try {
		const res = await apiFetch(
			`/api/plugins/${encodeURIComponent(plugin)}/${encodeURIComponent(fn)}`,
			{
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(payload)
			}
		);
		const text = await res.text();
		let data: unknown = null;
		try {
			data = text ? JSON.parse(text) : null;
		} catch {
			data = text;
		}
		if (res.ok) return { ok: true, status: res.status, data, error: null, kind: null };
		const message =
			data && typeof data === 'object' && 'error' in data && typeof data.error === 'string'
				? data.error
				: `The request failed (${res.status}).`;
		const kind = classifyStatus(res.status);
		const detail =
			data && typeof data === 'object' && 'detail' in data && typeof data.detail === 'string'
				? data.detail
				: undefined;
		pageError.current = { kind, status: res.status, message, plugin, fn, detail };
		return { ok: false, status: res.status, data, error: message, kind };
	} catch {
		const message = 'The server could not be reached.';
		pageError.current = { kind: 'network', status: 0, message, plugin, fn };
		return { ok: false, status: 0, data: null, error: message, kind: 'network' };
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

/** Total row count when the function answered with `{ total }` or `{ count }`; otherwise `undefined`. */
export function totalOf(data: unknown): number | undefined {
	if (!data || typeof data !== 'object' || Array.isArray(data)) return undefined;
	const object = data as Record<string, unknown>;
	for (const key of ['total', 'count'] as const) {
		const value = object[key];
		if (typeof value === 'number' && Number.isFinite(value) && value >= 0) {
			return Math.floor(value);
		}
	}
	return undefined;
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

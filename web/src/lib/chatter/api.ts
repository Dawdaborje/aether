import { apiFetch } from '$lib/api/client';

export interface TrackedField {
	id: string;
	name: string;
	label: string;
	type: string;
	options: { value: string; label?: string }[];
	/** Model a link field points at (`department` or `hr.employee`). */
	target?: string;
}

export interface ChatterMessage {
	id: string;
	kind: 'message' | 'note' | 'system' | 'change';
	author: string;
	author_label: string;
	body: string | null;
	mentions: string[];
	before: Record<string, unknown> | null;
	after: Record<string, unknown> | null;
	created_at: string;
	edited: boolean;
	deleted_at: string | null;
	record_deleted_at: string | null;
}

export interface Follower {
	actor: string;
	label: string;
	reason: string;
	muted: boolean;
}

export interface ChatterThread {
	enabled: true;
	config: { messages: boolean; notes: boolean; followers: boolean; track_changes: boolean };
	can: { message: boolean; note: boolean; follow: boolean; trash: boolean };
	me: string;
	member: boolean;
	tracked: TrackedField[];
	messages: ChatterMessage[];
	followers: Follower[];
	following: boolean;
	muted: boolean;
	trash_count: number;
}

export interface Person {
	actor: string;
	name: string;
}

/** What one request to the chatter API came back with. Never throws. */
export interface Answer<T> {
	ok: boolean;
	status: number;
	data: T | null;
	error: string | null;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<Answer<T>> {
	try {
		const res = await apiFetch(`/api/chatter/${path}`, {
			...init,
			headers: { 'Content-Type': 'application/json', ...init.headers }
		});
		const text = await res.text();
		let data: unknown = null;
		try {
			data = text ? JSON.parse(text) : null;
		} catch {
			data = null;
		}
		if (res.ok) return { ok: true, status: res.status, data: data as T, error: null };
		const message =
			data && typeof data === 'object' && 'error' in data && typeof data.error === 'string'
				? data.error
				: `The request failed (${res.status}).`;
		return { ok: false, status: res.status, data: null, error: message };
	} catch {
		return { ok: false, status: 0, data: null, error: 'The server could not be reached.' };
	}
}

const record = (plugin: string, model: string, key: string) =>
	`${encodeURIComponent(plugin)}/${encodeURIComponent(model)}/${encodeURIComponent(key)}`;

export const chatterApi = {
	thread: (plugin: string, model: string, key: string, trash = false) =>
		request<ChatterThread | { enabled: false }>(
			`${record(plugin, model, key)}${trash ? '?trash=true' : ''}`
		),
	post: (
		plugin: string,
		model: string,
		key: string,
		body: {
			kind: 'message' | 'note';
			body: string;
			mentions?: string[];
			link?: string;
			guest_name?: string;
		}
	) =>
		request<{ message: ChatterMessage }>(`${record(plugin, model, key)}/messages`, {
			method: 'POST',
			body: JSON.stringify(body)
		}),
	edit: (plugin: string, model: string, key: string, id: string, body: string) =>
		request<{ message: ChatterMessage }>(`${record(plugin, model, key)}/messages/${id}`, {
			method: 'PATCH',
			body: JSON.stringify({ body })
		}),
	remove: (plugin: string, model: string, key: string, id: string) =>
		request<{ ok: true }>(`${record(plugin, model, key)}/messages/${id}`, { method: 'DELETE' }),
	restore: (plugin: string, model: string, key: string, id: string) =>
		request<{ ok: true }>(`${record(plugin, model, key)}/messages/${id}/restore`, {
			method: 'POST'
		}),
	follow: (plugin: string, model: string, key: string, following: boolean, muted?: boolean) =>
		request<{ ok: true }>(`${record(plugin, model, key)}/follow`, {
			method: 'PUT',
			body: JSON.stringify({ following, muted })
		}),
	people: (q: string) => request<{ people: Person[] }>(`people?q=${encodeURIComponent(q)}`)
};

/** A tracked value as people read it: option labels, yes/no, link titles, and "empty" for nothing. */
export function showValue(
	field: TrackedField | undefined,
	value: unknown,
	titles: Record<string, string> = {}
): string {
	if (value === undefined || value === null || value === '') return 'empty';
	if (field?.type === 'bool') return value ? 'yes' : 'no';
	if (field?.type === 'select') {
		const option = field.options.find((option) => option.value === value);
		return option?.label ?? String(value);
	}
	if (field?.type === 'link' && typeof value === 'string') {
		const plain = value.replace(/[⟨⟩]/g, '');
		return titles[value] ?? titles[plain] ?? plain;
	}
	return typeof value === 'object' ? JSON.stringify(value) : String(value);
}

/** Link record ids in a thread, grouped by the model they point at. */
export function linkIdsByTarget(thread: ChatterThread): Map<string, string[]> {
	const byTarget = new Map<string, string[]>();
	for (const message of thread.messages) {
		if (message.kind !== 'change') continue;
		const keys = new Set([
			...Object.keys(message.before ?? {}),
			...Object.keys(message.after ?? {})
		]);
		for (const key of keys) {
			const field = thread.tracked.find((tracked) => tracked.id === key);
			if (field?.type !== 'link' || !field.target) continue;
			for (const raw of [message.before?.[key], message.after?.[key]]) {
				if (typeof raw !== 'string' || !raw.includes(':')) continue;
				const list = byTarget.get(field.target) ?? [];
				list.push(raw);
				byTarget.set(field.target, list);
			}
		}
	}
	return byTarget;
}

/** The lines of a change: what changed from what to what, with today's field labels. */
export function changeLines(
	message: ChatterMessage,
	tracked: TrackedField[],
	titles: Record<string, string> = {}
): { label: string; from: string; to: string }[] {
	const ids = new Set([...Object.keys(message.before ?? {}), ...Object.keys(message.after ?? {})]);
	return [...ids].map((id) => {
		const field = tracked.find((field) => field.id === id);
		return {
			label: field?.label ?? field?.name ?? 'A field that is no longer tracked',
			from: showValue(field, message.before?.[id], titles),
			to: showValue(field, message.after?.[id], titles)
		};
	});
}

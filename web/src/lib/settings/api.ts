import { apiFetch } from '$lib/api/client';
export type CatalogItem = {
	key: string;
	label: string;
	value: unknown;
	source: 'global' | 'org' | string;
	description?: string | null;
	long_description?: string | null;
	value_type: 'string' | 'boolean' | 'number' | 'list' | 'json' | 'secret' | string;
	/** An API key or password: `value` is always blank; `has_value` says whether one is saved. */
	secret?: boolean;
	has_value?: boolean;
};

export type CatalogGroup = {
	id: string;
	slug: string;
	label: string;
	color?: string | null;
	icon?: string | null;
	icon_type?: string | null;
	items: CatalogItem[];
};

export type SettingsCatalog = {
	groups: CatalogGroup[];
	org?: string | null;
};

export async function fetchSettingsCatalog(): Promise<SettingsCatalog> {
	const res = await apiFetch('/api/settings/catalog');
	if (!res.ok) {
		throw new Error('Failed to load settings');
	}
	return res.json();
}

export async function updateSetting(key: string, value: unknown): Promise<CatalogItem> {
	const res = await apiFetch(`/api/settings/${encodeURIComponent(key)}`, {
		method: 'PUT',
		credentials: 'include',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ value })
	});
	const body = await res.json().catch(() => ({}));
	if (!res.ok) {
		throw new Error(body.error ?? 'Failed to save setting');
	}
	return {
		key: body.key,
		label: key,
		value: body.value,
		source: body.source,
		secret: body.secret === true,
		has_value: body.has_value === true,
		value_type: body.secret === true
			? 'secret'
			: Array.isArray(body.value)
			? 'list'
			: typeof body.value === 'boolean'
				? 'boolean'
				: typeof body.value === 'number'
					? 'number'
					: typeof body.value === 'object'
						? 'json'
						: 'string'
	};
}

export type CatalogItem = {
	key: string;
	label: string;
	value: unknown;
	source: 'global' | 'org' | string;
	description?: string | null;
	long_description?: string | null;
	value_type: 'string' | 'boolean' | 'number' | 'list' | 'json' | string;
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
	const res = await fetch('/api/settings/catalog', { credentials: 'include' });
	if (!res.ok) {
		throw new Error('Failed to load settings');
	}
	return res.json();
}

export async function updateSetting(key: string, value: unknown): Promise<CatalogItem> {
	const res = await fetch(`/api/settings/${encodeURIComponent(key)}`, {
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
		value_type: Array.isArray(body.value)
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

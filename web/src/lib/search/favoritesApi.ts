import { apiFetch } from '$lib/api/client';

export interface SearchFavorite {
	id: string;
	model: string;
	label: string;
	filter: Record<string, unknown>;
	q?: string | null;
	group_by: string[];
	is_default?: boolean;
	date_created?: string;
	date_updated?: string;
}

async function readError(res: Response, fallback: string): Promise<string> {
	try {
		const body = (await res.json()) as { error?: string };
		return body.error?.trim() || fallback;
	} catch {
		return fallback;
	}
}

export async function fetchSearchFavorites(model: string): Promise<SearchFavorite[]> {
	const res = await apiFetch(
		`/api/ui/search/favorites?model=${encodeURIComponent(model)}`
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Favorites could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { favorites: SearchFavorite[] };
	return body.favorites ?? [];
}

export async function createSearchFavorite(input: {
	model: string;
	label: string;
	filter: Record<string, unknown>;
	q?: string | null;
	group_by?: string[];
	is_default?: boolean;
}): Promise<SearchFavorite> {
	const res = await apiFetch('/api/ui/search/favorites', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(input)
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Favorite could not be saved (${res.status}).`));
	}
	const body = (await res.json()) as { favorite: SearchFavorite };
	return body.favorite;
}

export async function deleteSearchFavorite(id: string): Promise<void> {
	const res = await apiFetch(`/api/ui/search/favorites/${encodeURIComponent(id)}`, {
		method: 'DELETE'
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Favorite could not be deleted (${res.status}).`));
	}
}

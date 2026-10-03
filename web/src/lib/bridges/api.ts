import { apiFetch } from '$lib/api/client';

/** One bridge of the catalog (seeded by `aether --seed`). */
export interface Bridge {
	feature_key: string;
	name: string;
	label: string;
	description: string | null;
	/** The category code, e.g. `auth_and_identity`. */
	category: string;
	version: string | null;
	is_builtin: boolean;
	enabled_globally: boolean;
}

export async function fetchBridges(): Promise<Bridge[]> {
	const res = await apiFetch('/api/ui/bridges');
	if (!res.ok) {
		const body = await res.json().catch(() => ({}));
		throw new Error(
			typeof body.error === 'string' ? body.error : `The bridges could not be loaded (${res.status}).`
		);
	}
	return ((await res.json()) as { bridges: Bridge[] }).bridges;
}

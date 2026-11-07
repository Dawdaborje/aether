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

/** Turn a bridge on or off for the whole platform. */
export async function setBridgeEnabled(featureKey: string, enabled: boolean): Promise<Bridge> {
	const res = await apiFetch(`/api/ui/bridges/${encodeURIComponent(featureKey)}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ enabled })
	});
	const body = await res.json().catch(() => ({}));
	if (!res.ok) {
		throw new Error(
			typeof body.error === 'string' ? body.error : `Could not update the bridge (${res.status}).`
		);
	}
	return (body as { bridge: Bridge }).bridge;
}

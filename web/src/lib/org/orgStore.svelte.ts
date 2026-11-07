import { apiFetch } from '$lib/api/client';

export interface OrgChoice {
	/** The organization's database name: what identifies it to the API. */
	db_name: string;
	name: string;
	/** The user belongs to it. */
	member: boolean;
	/** Uploaded logo URL when present; the UI shows a default mark otherwise. */
	logo_url?: string | null;
}

interface Overview {
	organizations: OrgChoice[];
	current: string | null;
}

async function fetchOverview(): Promise<Overview | null> {
	const res = await apiFetch('/api/auth/orgs');
	return res.ok ? ((await res.json()) as Overview) : null;
}

/** The installation's one organization. */
class OrgStore {
	organizations = $state<OrgChoice[]>([]);
	current = $state<string | null>(null);
	loaded = $state(false);

	/** The organizations the user belongs to. */
	get memberships(): OrgChoice[] {
		return this.organizations.filter((org) => org.member);
	}

	/** An installation has exactly one organization. Kept so pages can say so plainly. */
	get isSingle(): boolean {
		return true;
	}

	get currentName(): string | null {
		return this.organizations.find((org) => org.db_name === this.current)?.name ?? null;
	}

	/** Learn the installation's organization. */
	async load(): Promise<void> {
		const overview = await fetchOverview();
		if (!overview) return;
		this.organizations = overview.organizations;
		this.current = overview.current;
		this.loaded = true;
	}
}

export const orgStore = new OrgStore();

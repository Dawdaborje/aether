import { invalidateAll } from '$app/navigation';
import { apiFetch } from '$lib/api/client';
import { themeStore } from '$lib/theme';
import { getHeaderOrg, setHeaderOrg } from './selection';

export interface OrgChoice {
	/** The organization's database name: what identifies it to the API. */
	db_name: string;
	name: string;
	/**
	 * The user belongs to it. A developer can enter organizations they do not
	 * belong to; those are `false`.
	 */
	member: boolean;
}

/**
 * How this deployment decides which organization a request is for:
 * - `address`: the address (subdomain or path); the user is never asked.
 * - `header`: the browser sends `X-Org-Slug`; the choice is remembered here.
 * - `session`: the session remembers the choice.
 */
export type OrgMode = 'address' | 'header' | 'session';

interface Overview {
	mode: OrgMode;
	organizations: OrgChoice[];
	current: string | null;
	selection_required: boolean;
}

async function fetchOverview(): Promise<Overview | null> {
	const res = await apiFetch('/api/auth/orgs');
	return res.ok ? ((await res.json()) as Overview) : null;
}

class OrgStore {
	organizations = $state<OrgChoice[]>([]);
	mode = $state<OrgMode>('session');
	current = $state<string | null>(null);
	/** The user belongs to several organizations and none is chosen yet. */
	selectionRequired = $state(false);
	loaded = $state(false);
	modalOpen = $state(false);
	switching = $state(false);
	error = $state<string | null>(null);

	/** The organizations the user belongs to. */
	get memberships(): OrgChoice[] {
		return this.organizations.filter((org) => org.member);
	}

	/** The user can move between organizations (the address does not decide). */
	get canSwitch(): boolean {
		return this.mode !== 'address' && this.organizations.length > 1;
	}

	get currentName(): string | null {
		return this.organizations.find((org) => org.db_name === this.current)?.name ?? null;
	}

	/** Learn the user's organizations; asks them to choose when it has to. */
	async load(): Promise<void> {
		let overview = await fetchOverview();
		if (!overview) return;

		// A remembered choice that is no longer one of the user's organizations
		// (they were removed, or it was deleted) must not be sent again.
		const remembered = getHeaderOrg();
		if (remembered && !overview.organizations.some((org) => org.db_name === remembered)) {
			setHeaderOrg(null);
			overview = (await fetchOverview()) ?? overview;
		}

		this.organizations = overview.organizations;
		this.mode = overview.mode;
		this.current = overview.current;
		this.selectionRequired = overview.selection_required;
		this.loaded = true;
		if (overview.selection_required) this.modalOpen = true;
	}

	/** An API answered "organization selection required": ask now. */
	async requireSelection(): Promise<void> {
		await this.load();
		if (!this.selectionRequired) this.modalOpen = true;
	}

	openSwitcher(): void {
		this.error = null;
		this.modalOpen = true;
	}

	closeSwitcher(): void {
		// Choosing is mandatory while nothing is selected.
		if (!this.selectionRequired) this.modalOpen = false;
	}

	/** Work in `dbName` from now on, then reload what depends on it. */
	async choose(dbName: string): Promise<void> {
		this.switching = true;
		this.error = null;
		try {
			const res = await apiFetch('/api/auth/org', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ org: dbName })
			});
			if (!res.ok) {
				const body = await res.json().catch(() => ({}));
				throw new Error(body.error ?? `Could not switch organization (${res.status})`);
			}
			// In header mode the browser names the organization on every request.
			if (this.mode === 'header') setHeaderOrg(dbName);
			await this.load();
			this.modalOpen = false;
			await themeStore.loadFromApi('');
			await invalidateAll();
		} catch (err) {
			this.error = err instanceof Error ? err.message : 'Could not switch organization';
		} finally {
			this.switching = false;
		}
	}
}

export const orgStore = new OrgStore();

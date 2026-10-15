import { apiFetch } from '$lib/api/client';

export interface CompanyChoice {
	id: string;
	name: string;
	is_default: boolean;
}

interface Overview {
	available: boolean;
	current: CompanyChoice | null;
	companies: CompanyChoice[];
}

class CompanyStore {
	available = $state(false);
	current = $state<CompanyChoice | null>(null);
	companies = $state<CompanyChoice[]>([]);
	loaded = $state(false);
	switching = $state(false);
	error = $state<string | null>(null);

	get currentName(): string | null {
		return this.current?.name ?? null;
	}

	get canSwitch(): boolean {
		return this.available && this.companies.length > 1;
	}

	/** Load the working company for the current organization. Safe when the plugin is absent. */
	async load(): Promise<void> {
		this.error = null;
		try {
			const res = await apiFetch('/api/ui/company');
			if (!res.ok) {
				this.reset();
				this.loaded = true;
				return;
			}
			const body = (await res.json()) as Overview;
			this.available = Boolean(body.available);
			this.current = body.current ?? null;
			this.companies = Array.isArray(body.companies) ? body.companies : [];
			this.loaded = true;
		} catch {
			this.reset();
			this.loaded = true;
		}
	}

	/** Persist a new working company for this user in the current organization. */
	async choose(id: string): Promise<void> {
		if (!id || this.current?.id === id) return;
		this.switching = true;
		this.error = null;
		try {
			const res = await apiFetch('/api/ui/company', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ id })
			});
			if (!res.ok) {
				const body = await res.json().catch(() => ({}));
				throw new Error(
					(body as { error?: string }).error ?? `Could not switch company (${res.status})`
				);
			}
			const body = (await res.json()) as Overview;
			this.available = Boolean(body.available);
			this.current = body.current ?? null;
			this.companies = Array.isArray(body.companies) ? body.companies : [];
		} catch (err) {
			this.error = err instanceof Error ? err.message : 'Could not switch company';
		} finally {
			this.switching = false;
		}
	}

	/** Clear after org switch / leave so the next load is for the new org. */
	reset(): void {
		this.available = false;
		this.current = null;
		this.companies = [];
		this.loaded = false;
		this.error = null;
	}
}

export const companyStore = new CompanyStore();

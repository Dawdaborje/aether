import { authSession } from '$lib/auth/session.svelte';

/**
 * Small per-person interface choices a layout can offer: a folded sidebar, pinned pages, open
 * workspace tabs. They are kept in this browser, under the signed-in person's id, so two
 * people sharing a computer do not share them. (They do not follow the person to another
 * device.)
 */
class UiPrefs {
	// Plain (not `$state`): `get` runs inside `$derived`, where writing state is forbidden.
	// `version` is the reactive signal; it changes only in `set`, never while reading.
	private store: Record<string, unknown> = {};
	private version = $state(0);
	private loadedFor: string | null = null;

	private key(): string {
		return `aether-ui:${authSession.me?.user.id ?? 'anonymous'}`;
	}

	/** Read this person's values the first time (and again if the person changed). */
	private ensure() {
		const key = this.key();
		if (this.loadedFor === key) return;
		this.loadedFor = key;
		try {
			this.store = JSON.parse(localStorage.getItem(key) ?? '{}') as Record<string, unknown>;
		} catch {
			this.store = {};
		}
	}

	get<T>(name: string, fallback: T): T {
		if (typeof localStorage === 'undefined') return fallback;
		void this.version;
		this.ensure();
		return name in this.store ? (this.store[name] as T) : fallback;
	}

	set(name: string, value: unknown) {
		if (typeof localStorage === 'undefined') return;
		this.ensure();
		this.store = { ...this.store, [name]: value };
		this.version++;
		try {
			localStorage.setItem(this.key(), JSON.stringify(this.store));
		} catch {
			// Storage full or blocked: the choice lasts until the page is reloaded.
		}
	}
}

export const uiPrefs = new UiPrefs();

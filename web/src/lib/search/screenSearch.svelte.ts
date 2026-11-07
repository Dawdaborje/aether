import { goto } from '$app/navigation';
import { appsStore } from '$lib/apps/appsStore.svelte';
import { authSession } from '$lib/auth/session.svelte';
import { manageNavItems, navHref, pluginNavItems } from '$lib/components/layout/nav';
import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';
import { callPlugin } from '$lib/pages/pageContext.svelte';
import { buildScreenIndex, searchScreens, type ScreenEntry, type ScreenKind } from './screens';

/** A record a plugin's search function found for what was typed. */
export interface RecordHit {
	id: string;
	label: string;
	href: string;
	/** The app, and what the plugin adds (`Leave · Ada Lovelace`). */
	context: string;
}

/** The most records one provider adds to the results. */
const RECORDS_PER_APP = 5;

/** What the answer of a `search` function holds: `{ label, href, context? }` items with app paths. */
export function recordHits(app: string, answer: unknown): RecordHit[] {
	const list = Array.isArray(answer)
		? answer
		: answer && typeof answer === 'object' && Array.isArray((answer as { rows?: unknown }).rows)
			? (answer as { rows: unknown[] }).rows
			: [];
	const hits: RecordHit[] = [];
	for (const item of list) {
		if (!item || typeof item !== 'object') continue;
		const { label, href, context } = item as Record<string, unknown>;
		if (typeof label !== 'string' || !label.trim()) continue;
		if (typeof href !== 'string' || !href.startsWith('/') || href.startsWith('//')) continue;
		hits.push({
			id: `record:${href}`,
			label: label.trim().slice(0, 120),
			href,
			context: typeof context === 'string' && context.trim() ? `${app} · ${context.trim()}` : app
		});
		if (hits.length >= RECORDS_PER_APP) break;
	}
	return hits;
}

/** What the palette and the top-bar box share: the screens a person may open, and the palette's open state. */
class ScreenSearch {
	/** The palette (Ctrl/Cmd+K) is showing. */
	open = $state(false);

	/** Every screen this account may open; rebuilt when the apps or the permissions change. */
	index = $derived.by(() =>
		buildScreenIndex({
			apps: appsStore.apps ?? [],
			palette: appsStore.palette,
			manage: manageNavItems(
				authSession.isDeveloper,
				authSession.canManageUsers,
				authSession.canManageOrg,
				authSession.isOrgDeveloper,
				authSession.canManageRoles,
				pluginNavItems(appsStore.navLinks)
			)
		})
	);

	/** True until the apps list has loaded: only the general screens exist before that. */
	get loading(): boolean {
		return appsStore.apps === null;
	}

	find(query: string, limit = 30): ScreenEntry[] {
		return searchScreens(this.index, query, limit);
	}

	toggle(): void {
		this.open = !this.open;
	}

	/** Records the plugins' `search` functions found for the last query. */
	records = $state<RecordHit[]>([]);
	private searchToken = 0;

	/** Ask every plugin that offers record search for matches of `query` (two letters or more). */
	async findRecords(query: string): Promise<void> {
		const token = ++this.searchToken;
		const text = query.trim();
		const providers = appsStore.palette.filter((entry) => entry.search);
		if (text.length < 2 || providers.length === 0) {
			this.records = [];
			return;
		}
		const answers = await Promise.all(
			providers.map(async (provider) => {
				const dot = provider.search!.indexOf('.');
				const result = await callPlugin(
					provider.search!.slice(0, dot),
					provider.search!.slice(dot + 1),
					{ q: text, limit: RECORDS_PER_APP }
				);
				return result.ok ? recordHits(provider.app, result.data) : [];
			})
		);
		// A newer query has started; its answer wins.
		if (token === this.searchToken) this.records = answers.flat();
	}

	/** Run a palette command's function; it may answer `{ href }` to open a page or `{ message }`. */
	async run(fn: string): Promise<void> {
		const dot = fn.indexOf('.');
		const result = await callPlugin(fn.slice(0, dot), fn.slice(dot + 1));
		if (!result.ok) {
			notificationsStore.toast('error', result.error ?? 'The command failed.');
			return;
		}
		const answer = result.data as { href?: unknown; message?: unknown } | string | null;
		if (typeof answer === 'string' && answer.trim()) notificationsStore.toast('success', answer);
		else if (answer && typeof answer === 'object') {
			if (typeof answer.message === 'string') notificationsStore.toast('success', answer.message);
			if (
				typeof answer.href === 'string' &&
				answer.href.startsWith('/') &&
				!answer.href.startsWith('//')
			) {
				// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref() applies the base path
				await goto(navHref(answer.href));
			}
		}
	}

	async go(entry: ScreenEntry | RecordHit): Promise<void> {
		this.open = false;
		if ('run' in entry && entry.run) {
			await this.run(entry.run);
			return;
		}
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref() applies the base path
		await goto(navHref(entry.href));
	}
}

export const screenSearch = new ScreenSearch();

/** The headings the results are grouped under, in display order. */
export const KIND_HEADINGS: { kind: ScreenKind; heading: string }[] = [
	{ kind: 'command', heading: 'Commands' },
	{ kind: 'app', heading: 'Apps' },
	{ kind: 'page', heading: 'Pages' },
	{ kind: 'manage', heading: 'Manage' },
	{ kind: 'general', heading: 'General' }
];

/** Results split under their headings, keeping the best-first order inside each; empty groups are left out. */
export function groupByKind(entries: ScreenEntry[]): { heading: string; entries: ScreenEntry[] }[] {
	return KIND_HEADINGS.map(({ kind, heading }) => ({
		heading,
		entries: entries.filter((entry) => entry.kind === kind)
	})).filter((group) => group.entries.length > 0);
}

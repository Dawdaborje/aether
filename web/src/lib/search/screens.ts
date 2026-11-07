import type {
	AppMenuItem,
	AppNavLink,
	AppPaletteEntry,
	AppTileData
} from '$lib/components/apps/types';
import type { NavItem } from '$lib/theme/types';

/** Where a screen comes from; the palette groups its results by this. */
export type ScreenKind = 'command' | 'app' | 'page' | 'manage' | 'general';

/** One screen a person can open: what the searcher lists and matches. */
export interface ScreenEntry {
	/** Stable key: the href. */
	id: string;
	label: string;
	/** An in-app path starting with `/` (the base path is added when navigating). */
	href: string;
	kind: ScreenKind;
	/** The app or section it belongs to, shown beside the label and matched too. */
	context?: string;
	icon?: string | null;
	/** Extra words that find it (an app's description). */
	keywords?: string;
	/** A palette entry's keyboard shortcut, normalised (`alt+n`, `mod+shift+e`). */
	shortcut?: string;
	/** A palette entry that runs `plugin.function` instead of opening `href`. */
	run?: string;
}

export interface ScreenSources {
	apps: AppTileData[];
	/** `[app] nav = true` plugins. */
	navLinks?: AppNavLink[];
	/** The management links this account may open (Users, Settings, ...). */
	manage?: NavItem[];
	/** What plugins add to the palette (`[[app.palette]]`), already filtered by role. */
	palette?: AppPaletteEntry[];
}

/** Screens that exist for everyone who is signed in. */
const GENERAL: ScreenEntry[] = [
	{ id: '/apps', label: 'All apps', href: '/apps', kind: 'general', icon: 'layout-grid' },
	{ id: '/profile', label: 'Profile', href: '/profile', kind: 'general', icon: 'user' }
];

const inApp = (href: string | null | undefined): href is string => !!href && href.startsWith('/');

/**
 * Every screen the person may open, from what the server already lists for them (so roles are
 * respected): app tiles, each app's pages, management links and a few general screens. The first
 * entry for a path wins.
 */
export function buildScreenIndex(sources: ScreenSources): ScreenEntry[] {
	const out: ScreenEntry[] = [];
	const seen = new Set<string>();
	const add = (entry: ScreenEntry) => {
		if (!inApp(entry.href) || seen.has(entry.href)) return;
		seen.add(entry.href);
		out.push(entry);
	};

	// Palette entries come first and may point at a screen that is also listed below.
	for (const entry of sources.palette ?? []) {
		// Record searches are not listed; the palette asks them as you type.
		if (entry.search) continue;
		if (entry.function) {
			out.push({
				id: `command:run:${entry.function}#${entry.label}`,
				label: entry.label,
				href: '',
				run: entry.function,
				kind: 'command',
				context: entry.app,
				icon: entry.icon,
				keywords: entry.keywords ?? undefined,
				shortcut: entry.shortcut ?? undefined
			});
			continue;
		}
		if (!entry.href || !inApp(entry.href)) continue;
		out.push({
			id: `command:${entry.href}#${entry.label}`,
			label: entry.label,
			href: entry.href,
			kind: 'command',
			context: entry.app,
			icon: entry.icon,
			keywords: entry.keywords ?? undefined,
			shortcut: entry.shortcut ?? undefined
		});
	}
	for (const app of sources.apps) {
		add({
			id: app.route,
			label: app.label,
			href: app.route,
			kind: 'app',
			icon: app.icon,
			keywords: app.description ?? undefined
		});
	}
	for (const app of sources.apps) {
		for (const menu of app.menus ?? []) add(pageEntry(app.label, menu));
	}
	for (const link of sources.navLinks ?? []) {
		add({ id: link.href, label: link.label, href: link.href, kind: 'manage', icon: link.icon });
	}
	for (const item of sources.manage ?? []) {
		if (item.href) {
			add({ id: item.href, label: item.label, href: item.href, kind: 'manage', icon: item.icon });
		}
	}
	for (const entry of GENERAL) add(entry);
	return out;
}

function pageEntry(appLabel: string, menu: AppMenuItem): ScreenEntry {
	return {
		id: menu.href,
		label: menu.label,
		href: menu.href,
		kind: 'page',
		context: menu.group ? `${appLabel} · ${menu.group}` : appLabel,
		icon: menu.icon ?? null
	};
}

const norm = (text: string) => text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();

/** The letters of `needle` appear in `hay` in order (a typo-tolerant last resort). */
function inOrder(needle: string, hay: string): boolean {
	let at = 0;
	for (const ch of hay) {
		if (ch === needle[at]) at += 1;
		if (at === needle.length) return true;
	}
	return false;
}

/** How well one word of the query matches a field; 0 is no match. */
function wordScore(word: string, field: string): number {
	if (!field) return 0;
	if (field === word) return 100;
	if (field.startsWith(word)) return 80;
	if (field.split(/[\s\-_/·.]+/).some((part) => part.startsWith(word))) return 60;
	if (field.includes(word)) return 40;
	if (word.length >= 3 && inOrder(word, field)) return 15;
	return 0;
}

/**
 * How well `entry` matches `query`: every word of the query has to match somewhere (the label
 * counts most, then the app or section, then the keywords); 0 means it does not match.
 */
export function scoreScreen(entry: ScreenEntry, query: string): number {
	const words = norm(query).split(/\s+/).filter(Boolean);
	if (words.length === 0) return 1;
	const label = norm(entry.label);
	const context = norm(entry.context ?? '');
	const keywords = norm(entry.keywords ?? '');
	let total = 0;
	for (const word of words) {
		const best = Math.max(
			wordScore(word, label) * 3,
			wordScore(word, context) * 1.5,
			// Keywords and the path only help with whole-word and substring hits, not loose letters.
			Math.min(wordScore(word, keywords), 40),
			Math.min(wordScore(word, norm(entry.href)), 40)
		);
		if (best === 0) return 0;
		total += best;
	}
	return total;
}

const KIND_ORDER: Record<ScreenKind, number> = {
	command: 0,
	app: 1,
	page: 2,
	manage: 3,
	general: 4
};

/**
 * The screens that match `query`, best first (ties keep the index order, apps before pages).
 * An empty query lists the first `limit` screens in index order.
 */
export function searchScreens(index: ScreenEntry[], query: string, limit = 30): ScreenEntry[] {
	if (!query.trim()) return index.slice(0, limit);
	return index
		.map((entry, order) => ({ entry, order, score: scoreScreen(entry, query) }))
		.filter((hit) => hit.score > 0)
		.sort(
			(a, b) =>
				b.score - a.score ||
				KIND_ORDER[a.entry.kind] - KIND_ORDER[b.entry.kind] ||
				a.order - b.order
		)
		.slice(0, limit)
		.map((hit) => hit.entry);
}

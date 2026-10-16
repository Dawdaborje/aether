import { getContext, setContext, untrack } from 'svelte';
import type { FilterPreset, GroupByOption, SearchChip } from '$lib/search/types';

const CHROME = Symbol('aether.pageChrome');

/**
 * Control panel chrome for a plugin page: New under the breadcrumbs,
 * faceted search on the right. The primary list registers into this; PageShell
 * reads it to fill ControlPanel.
 */
export class PageChrome {
	createHref = $state<string | null>(null);
	/**
	 * Where a form puts its Save and buttons (under the page name) and its state steps (on the
	 * right of that bar): containers PageShell renders in the control panel. `barOwner` is the
	 * one form that claimed them, so a form in a pop-up does not take them over.
	 */
	barActions = $state<HTMLElement | null>(null);
	barStatus = $state<HTMLElement | null>(null);
	barOwner = $state<object | null>(null);
	/** The page shows a title bar (control panel) that forms can use. */
	hasBar = $state(false);
	/** The page's main form holds changes that are not saved. */
	dirty = $state(false);
	createLabel = $state('New');
	searchable = $state(false);
	/** Free-text draft in the search bar (before Enter commits a `_q` chip). */
	query = $state('');
	/** Model name for favorites scoping. */
	model = $state<string | null>(null);
	/**
	 * Models this page's views care about for live `data.changed` refresh.
	 * List/form/kanban/calendar register here so PageShell can match SSE events
	 * even when `<page model>` is omitted.
	 */
	refreshModels = $state<string[]>([]);
	/** Fields `db::search` expands free-text over. */
	searchFields = $state<string[]>([]);
	/** Human labels for `searchFields` (same order), for placeholders / chips. */
	searchFieldLabels = $state<string[]>([]);
	/** Declared `<filter>` presets for the Filters menu. */
	filterPresets = $state<FilterPreset[]>([]);
	/** Declared `<groupby>` options. */
	groupByOptions = $state<GroupByOption[]>([]);
	/** Active search chips. */
	chips = $state<SearchChip[]>([]);
	/** Active group-by field names. */
	groupBy = $state<string[]>([]);
	/** Bumps when search state changes so the list reloads. */
	searchTick = $state(0);

	/**
	 * Register a model for SSE-driven page refresh; returns an unregister.
	 * Reads and writes run inside `untrack` so a caller's `$effect` does not
	 * subscribe to `refreshModels`. Subscribing would reschedule that effect,
	 * its cleanup would drop the model, and the effect would add it again.
	 */
	registerRefreshModel(model: string | null | undefined): () => void {
		const name = model?.trim() ?? '';
		if (!name) return () => {};
		untrack(() => {
			if (!this.refreshModels.includes(name)) {
				this.refreshModels = [...this.refreshModels, name];
			}
		});
		return () => {
			untrack(() => {
				if (!this.refreshModels.includes(name)) return;
				this.refreshModels = this.refreshModels.filter((row) => row !== name);
			});
		};
	}

	setChips(next: SearchChip[]) {
		this.chips = next;
		this.searchTick += 1;
	}

	setGroupBy(next: string[]) {
		this.groupBy = next;
		this.searchTick += 1;
	}

	addChip(chip: SearchChip) {
		this.chips = [...this.chips, chip];
		this.searchTick += 1;
	}

	removeChip(id: string) {
		this.chips = this.chips.filter((chip) => chip.id !== id);
		this.searchTick += 1;
	}

	clear(): void {
		this.createHref = null;
		this.createLabel = 'New';
		this.searchable = false;
		this.query = '';
		this.model = null;
		this.refreshModels = [];
		this.searchFields = [];
		this.searchFieldLabels = [];
		this.filterPresets = [];
		this.groupByOptions = [];
		this.chips = [];
		this.groupBy = [];
		this.searchTick = 0;
	}
}

export function setPageChrome(chrome: PageChrome): void {
	setContext(CHROME, chrome);
}

export function getPageChrome(): PageChrome | undefined {
	return getContext<PageChrome | undefined>(CHROME);
}

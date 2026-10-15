<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import XIcon from '@lucide/svelte/icons/x';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import type { PageChrome } from '$lib/pages/pageChrome.svelte';
	import FavoritesMenu from './FavoritesMenu.svelte';
	import FilterMenu from './FilterMenu.svelte';
	import GroupByMenu from './GroupByMenu.svelte';
	import { Q_FIELD, makeChip } from './filterCodec';
	import { decodeSearchParams, mergeSearchIntoUrl } from './urlState';

	let { chrome }: { chrome: PageChrome } = $props();

	let draft = $state('');
	let hydrated = $state(false);
	let lastPath = $state('');

	// Drop search state when navigating to another page (PageShell is reused under [...slug]).
	$effect(() => {
		const path = page.url.pathname;
		if (path === lastPath) return;
		const previous = lastPath;
		lastPath = path;
		if (!previous) return;
		draft = '';
		chrome.query = '';
		chrome.chips = [];
		chrome.groupBy = [];
		chrome.searchTick += 1;
		hydrated = false;
	});

	// Hydrate from URL once when search becomes available (or after a path change).
	$effect(() => {
		if (!chrome.searchable || hydrated) return;
		const state = decodeSearchParams(page.url.searchParams);
		if (state.chips.length > 0 || state.groupBy.length > 0) {
			chrome.chips = state.chips;
			chrome.groupBy = state.groupBy;
			chrome.searchTick += 1;
		} else {
			chrome.chips = [];
			chrome.groupBy = [];
		}
		hydrated = true;
	});

	// Keep the URL in sync with chips / group_by.
	$effect(() => {
		if (!hydrated || !chrome.searchable) return;
		void chrome.searchTick;
		const next = mergeSearchIntoUrl(page.url, chrome.chips, chrome.groupBy);
		if (next.search !== page.url.search) {
			void goto(`${next.pathname}${next.search}${next.hash}`, {
				replaceState: true,
				keepFocus: true,
				noScroll: true
			});
		}
	});

	const searchHint = $derived.by(() => {
		const labels = chrome.searchFieldLabels.filter(Boolean);
		if (labels.length === 0) {
			return chrome.searchFields.map((f) => f.replaceAll('_', ' ')).filter(Boolean);
		}
		return labels;
	});

	const searchPlaceholder = $derived(
		chrome.chips.length === 0
			? searchHint.length > 0
				? `Search ${searchHint.slice(0, 3).join(', ')}${searchHint.length > 3 ? '…' : ''}…`
				: 'Search…'
			: 'Add filter…'
	);

	function commitDraft() {
		const value = draft.trim();
		if (!value) return;
		const withoutQ = chrome.chips.filter((chip) => chip.field !== Q_FIELD);
		const where =
			searchHint.length > 0 ? ` in ${searchHint.slice(0, 4).join(', ')}` : '';
		chrome.setChips([
			...withoutQ,
			makeChip({
				field: Q_FIELD,
				op: 'like',
				value,
				label: `Contains “${value}”${where}`
			})
		]);
		draft = '';
		chrome.query = '';
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter') {
			event.preventDefault();
			commitDraft();
			return;
		}
		if (event.key === 'Backspace' && !draft && chrome.chips.length > 0) {
			const last = chrome.chips[chrome.chips.length - 1];
			if (last) chrome.removeChip(last.id);
		}
	}

	// Live draft for client-side lists and server-mode debounced q.
	$effect(() => {
		chrome.query = draft;
	});
</script>

<div class="flex w-full max-w-3xl flex-wrap items-center justify-end gap-2">
	<div
		class="flex min-h-9 min-w-[16rem] flex-1 flex-wrap items-center gap-1.5 rounded-md border border-input bg-background px-2 py-1 shadow-xs focus-within:ring-2 focus-within:ring-ring/30"
	>
		<SearchIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
		{#each chrome.chips as chip (chip.id)}
			<span
				class="inline-flex max-w-[14rem] items-center gap-1 rounded-md bg-primary/10 px-1.5 py-0.5 text-xs font-medium text-primary"
			>
				<span class="truncate">{chip.label}</span>
				<button
					type="button"
					class="rounded p-0.5 hover:bg-primary/15"
					aria-label={`Remove ${chip.label}`}
					onclick={() => chrome.removeChip(chip.id)}
				>
					<XIcon class="size-3" />
				</button>
			</span>
		{/each}
		<input
			bind:value={draft}
			type="search"
			placeholder={searchPlaceholder}
			aria-label={searchPlaceholder}
			title={
				searchHint.length > 0
					? `Free text matches any of: ${searchHint.join(', ')}`
					: 'Free-text search'
			}
			class="min-w-[8rem] flex-1 bg-transparent py-1 text-sm outline-none placeholder:text-muted-foreground"
			onkeydown={onKeydown}
		/>
	</div>
	<FilterMenu {chrome} />
	<GroupByMenu {chrome} />
	<FavoritesMenu {chrome} />
</div>

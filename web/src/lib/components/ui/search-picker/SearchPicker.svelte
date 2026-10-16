<script lang="ts">
	import type { Component } from 'svelte';
	import CheckIcon from '@lucide/svelte/icons/check';
	import ChevronsUpDownIcon from '@lucide/svelte/icons/chevrons-up-down';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import type { SearchPickerItem } from './types';

	const DEFAULT_VISIBLE = 5;
	const SEARCH_VISIBLE = 50;

	let {
		items,
		value = $bindable(null as string | null),
		preferId = null,
		label = '',
		placeholder = 'Select…',
		searchPlaceholder = 'Search…',
		entityLabel = 'items',
		icon: Icon = undefined,
		clearable = false,
		creatable = false,
		creating = false,
		class: className = '',
		onchange,
		onCreate
	}: {
		items: SearchPickerItem[];
		value?: string | null;
		/** Prefer this id near the top of the default list. */
		preferId?: string | null;
		label?: string;
		placeholder?: string;
		searchPlaceholder?: string;
		/** Plural noun used in empty / overflow hints (e.g. "plugins"). */
		entityLabel?: string;
		icon?: Component<{ class?: string }>;
		/** Show a control to clear the selection. */
		clearable?: boolean;
		/** Offer to create a new item when the search has no exact label match. */
		creatable?: boolean;
		/** True while `onCreate` is in flight. */
		creating?: boolean;
		class?: string;
		onchange?: (id: string) => void;
		onCreate?: (query: string) => void | Promise<void>;
	} = $props();

	let open = $state(false);
	let query = $state('');
	let rootEl = $state<HTMLDivElement | null>(null);

	const selected = $derived(items.find((item) => item.id === value) ?? null);

	const ranked = $derived.by(() => {
		const q = query.trim().toLowerCase();
		const matched = q
			? items.filter(
					(item) =>
						item.label.toLowerCase().includes(q) ||
						item.id.toLowerCase().includes(q) ||
						(item.sublabel?.toLowerCase().includes(q) ?? false)
				)
			: [...items];
		matched.sort((a, b) => {
			const score = (item: SearchPickerItem) =>
				(item.id === value ? 0 : 2) + (item.id === preferId ? 1 : 2);
			const byScore = score(a) - score(b);
			if (byScore !== 0) return byScore;
			return a.label.localeCompare(b.label);
		});
		return matched;
	});

	const shown = $derived(ranked.slice(0, query.trim() ? SEARCH_VISIBLE : DEFAULT_VISIBLE));
	const hiddenCount = $derived(Math.max(0, ranked.length - shown.length));
	const createQuery = $derived(query.trim());
	const canCreate = $derived(
		creatable &&
			Boolean(onCreate) &&
			createQuery.length > 0 &&
			!items.some((item) => item.label.toLowerCase() === createQuery.toLowerCase())
	);

	function pick(id: string) {
		value = id;
		open = false;
		query = '';
		onchange?.(id);
	}

	function clear() {
		value = null;
		open = false;
		query = '';
		onchange?.('');
	}

	async function createFromQuery() {
		if (!canCreate || creating || !onCreate) return;
		await onCreate(createQuery);
		open = false;
		query = '';
	}

	function onWindowPointerDown(event: PointerEvent) {
		if (!open || !rootEl) return;
		if (rootEl.contains(event.target as Node)) return;
		open = false;
		query = '';
	}
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="relative w-full {className}" bind:this={rootEl}>
	{#if label}
		<span class="mb-1 block text-xs text-muted-foreground">{label}</span>
	{/if}
	<Button
		type="button"
		variant="outline"
		class="h-9 w-full justify-between gap-2 px-3 font-normal"
		aria-expanded={open}
		aria-haspopup="listbox"
		onclick={() => (open = !open)}
	>
		<span class="flex min-w-0 items-center gap-2">
			{#if Icon}
				<Icon class="size-3.5 shrink-0 text-muted-foreground" />
			{/if}
			<span class="truncate">
				{#if selected}
					{selected.label}
				{:else}
					{placeholder}
				{/if}
			</span>
		</span>
		<ChevronsUpDownIcon class="size-3.5 shrink-0 opacity-60" />
	</Button>

	{#if open}
		<div
			class="absolute top-full right-0 left-0 z-50 mt-1 overflow-hidden rounded-md border border-border bg-popover text-popover-foreground shadow-md"
			role="listbox"
			aria-label={label || entityLabel}
		>
			<div class="relative border-b border-border p-2">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-4 size-3.5 -translate-y-1/2 text-muted-foreground"
				/>
				<Input class="h-8 pl-8" placeholder={searchPlaceholder} bind:value={query} autofocus />
			</div>
			<ul class="max-h-64 overflow-y-auto p-1">
				{#if clearable && value}
					<li>
						<button
							type="button"
							class="mb-1 flex w-full items-center rounded-sm px-2 py-2 text-left text-sm text-muted-foreground transition hover:bg-accent"
							onclick={clear}
						>
							Clear selection
						</button>
					</li>
				{/if}
				{#if shown.length === 0 && !canCreate}
					<li class="px-2 py-3 text-center text-sm text-muted-foreground">
						No {entityLabel} match.
					</li>
				{:else}
					{#each shown as item (item.id)}
						<li>
							<button
								type="button"
								role="option"
								aria-selected={item.id === value}
								class="flex w-full items-center gap-2 rounded-sm px-2 py-2 text-left text-sm transition hover:bg-accent"
								onclick={() => pick(item.id)}
							>
								<span class="flex min-w-0 flex-1 flex-col">
									<span class="truncate font-medium">{item.label}</span>
									{#if item.sublabel}
										<span class="truncate font-mono text-xs text-muted-foreground">
											{item.sublabel}
										</span>
									{/if}
								</span>
								{#if item.id === value}
									<CheckIcon class="size-3.5 shrink-0 text-primary" />
								{/if}
							</button>
						</li>
					{/each}
				{/if}
				{#if canCreate}
					<li>
						<button
							type="button"
							class="mt-0.5 flex w-full items-center gap-2 rounded-sm px-2 py-2 text-left text-sm transition hover:bg-accent disabled:opacity-60"
							disabled={creating}
							onclick={() => void createFromQuery()}
						>
							<PlusIcon class="size-3.5 shrink-0 text-muted-foreground" />
							<span class="truncate">
								{creating ? 'Creating…' : `Create “${createQuery}”`}
							</span>
						</button>
					</li>
				{/if}
			</ul>
			{#if !query.trim() && items.length > DEFAULT_VISIBLE}
				<p class="border-t border-border px-3 py-2 text-xs text-muted-foreground">
					Showing {DEFAULT_VISIBLE} of {items.length}. Type to search.
				</p>
			{:else if query.trim() && hiddenCount > 0}
				<p class="border-t border-border px-3 py-2 text-xs text-muted-foreground">
					{hiddenCount} more match — refine your search.
				</p>
			{/if}
		</div>
	{/if}
</div>

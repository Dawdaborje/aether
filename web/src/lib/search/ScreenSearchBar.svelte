<script lang="ts">
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { screenSearch } from './screenSearch.svelte';
	import { isMacPlatform, isTyping } from './shortcuts';

	/** `/` (outside a field) puts the cursor in the box. */
	let { slash = false }: { slash?: boolean } = $props();

	let query = $state('');
	let focused = $state(false);
	let active = $state(0);
	let input = $state<HTMLInputElement | null>(null);

	const results = $derived(query.trim() ? screenSearch.find(query, 8) : []);
	const showing = $derived(focused && query.trim().length > 0);
	const listId = 'screen-search-results';

	// A new query starts at the best match.
	$effect(() => {
		void query;
		active = 0;
	});

	async function choose(index: number) {
		const entry = results[index];
		if (!entry) return;
		query = '';
		input?.blur();
		await screenSearch.go(entry);
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			if (results.length) active = (active + 1) % results.length;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			if (results.length) active = (active - 1 + results.length) % results.length;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			void choose(active);
		} else if (event.key === 'Escape') {
			query = '';
			input?.blur();
		}
	}

	const isMac = isMacPlatform();

	function onWindowKeydown(event: KeyboardEvent) {
		if (!slash || event.key !== '/' || event.defaultPrevented || isTyping(event.target)) return;
		event.preventDefault();
		input?.focus();
	}
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div class="relative hidden w-56 sm:block lg:w-72" data-part="search.field">
	<AppIcon
		name="search"
		class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-region-muted-foreground"
	/>
	<input
		bind:this={input}
		bind:value={query}
		type="search"
		role="combobox"
		aria-label="Find a screen or command"
		aria-expanded={showing}
		aria-controls={listId}
		aria-activedescendant={showing && results.length ? `${listId}-${active}` : undefined}
		autocomplete="off"
		placeholder="Search…"
		onfocus={() => (focused = true)}
		onblur={() => (focused = false)}
		onkeydown={onKeydown}
		class="h-8 w-full rounded-field border border-region-border bg-region-hover/40 pr-12 pl-8 text-[13px] text-region-foreground transition outline-none placeholder:text-region-muted-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
	/>
	{#if !query}
		<kbd
			class="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 rounded border border-region-border px-1 text-[10px] text-region-muted-foreground"
			>{slash ? '/' : isMac ? '⌘ K' : 'Ctrl K'}</kbd
		>
	{/if}

	{#if showing}
		<ul
			id={listId}
			role="listbox"
			class="absolute top-full right-0 left-0 z-50 mt-1 max-h-80 overflow-y-auto rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
		>
			{#each results as entry, i (entry.id)}
				{@const Icon = iconRef(entry.icon)}
				<!-- mousedown, not click: the input's blur would close the list before a click lands. -->
				<li
					id="{listId}-{i}"
					role="option"
					aria-selected={i === active}
					onmousedown={(event) => {
						event.preventDefault();
						void choose(i);
					}}
					onmouseenter={() => (active = i)}
					class="flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm aria-selected:bg-muted"
				>
					{#if Icon}<AppIcon name={Icon} class="size-4 shrink-0 text-muted-foreground" />{/if}
					<span class="truncate">{entry.label}</span>
					{#if entry.context}
						<span class="truncate text-xs text-muted-foreground">{entry.context}</span>
					{/if}
				</li>
			{:else}
				<li class="px-2 py-1.5 text-sm text-muted-foreground">No screen matches “{query}”.</li>
			{/each}
		</ul>
	{/if}
</div>

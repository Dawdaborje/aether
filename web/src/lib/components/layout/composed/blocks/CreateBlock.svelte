<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import type { LayoutBlock } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import { textProp } from '../visibility';

	let { block }: { block: LayoutBlock } = $props();
	const label = $derived(textProp(block, 'label', 'New'));
	const fab = $derived(textProp(block, 'style', 'button') === 'fab');
	let open = $state(false);

	/** What apps offer under "New": their palette entries marked `create = true`. */
	const entries = $derived(appsStore.palette.filter((entry) => entry.create && entry.href));

	$effect(() => {
		void page.url.pathname;
		open = false;
	});

	function pick(href: string) {
		open = false;
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref() applies the base path
		void goto(navHref(href));
	}
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="relative {fab ? 'fixed right-4 bottom-4 z-40' : ''}" data-part="shell.create">
	<button
		type="button"
		aria-haspopup="menu"
		aria-expanded={open}
		onclick={() => (open = !open)}
		class="flex items-center gap-2 bg-primary text-sm font-medium text-primary-foreground transition outline-none hover:bg-primary-hover focus-visible:ring-2 focus-visible:ring-ring {fab
			? 'size-14 justify-center rounded-full shadow-lg'
			: 'h-8 rounded-button px-3'}"
		aria-label={fab ? label : undefined}
	>
		<AppIcon name="plus" class="size-4" />
		{#if !fab}{label}{/if}
	</button>
	{#if open}
		<ul
			role="menu"
			class="absolute z-50 min-w-48 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md {fab
				? 'right-0 bottom-full mb-2'
				: 'top-full left-0 mt-1'}"
		>
			{#each entries as entry (entry.plugin + entry.label)}
				<li role="none">
					<button
						type="button"
						role="menuitem"
						onclick={() => pick(entry.href!)}
						class="flex w-full items-center justify-between gap-4 rounded-sm px-2.5 py-1.5 text-left text-sm hover:bg-accent"
					>
						{entry.label}<span class="text-xs text-muted-foreground">{entry.app}</span>
					</button>
				</li>
			{:else}
				<li class="px-2.5 py-1.5 text-sm text-muted-foreground">Nothing to create here.</li>
			{/each}
		</ul>
	{/if}
</div>

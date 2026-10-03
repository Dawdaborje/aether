<script lang="ts">
	import { page } from '$app/state';
	import SearchIcon from '@lucide/svelte/icons/search';
	import SettingField from '$lib/components/pages/settings/SettingField.svelte';
	import { getContext } from 'svelte';
	import type { CatalogGroup, CatalogItem } from '$lib/settings/api';

	type SettingsCtx = {
		groups: CatalogGroup[];
		setGroups: (groups: CatalogGroup[]) => void;
	};

	const ctx = getContext<SettingsCtx>('settings-groups');

	const group = $derived(ctx?.groups?.find((g) => g.slug === page.params.group) ?? null);

	let query = $state('');
	const items = $derived(
		(group?.items ?? []).filter((item) => {
			const needle = query.trim().toLowerCase();
			if (!needle) return true;
			return [item.label, item.key, item.description ?? ''].some((text) =>
				text.toLowerCase().includes(needle)
			);
		})
	);

	function onSaved(updated: CatalogItem) {
		if (!ctx || !group) return;
		ctx.setGroups(
			ctx.groups.map((g) =>
				g.slug !== group.slug
					? g
					: {
							...g,
							items: g.items.map((item) =>
								item.key === updated.key ? { ...item, ...updated } : item
							)
						}
			)
		);
	}
</script>

{#if !group}
	<div class="rounded-md border border-dashed border-border bg-card p-8 text-center">
		<p class="font-medium">Settings group not found</p>
		<p class="mt-1 text-sm text-muted-foreground">
			There is no group named <code class="text-xs">{page.params.group}</code>.
		</p>
	</div>
{:else}
	<section class="space-y-5">
		<div class="flex flex-wrap items-end justify-between gap-4">
			<div class="space-y-1">
				<h2 class="text-lg font-semibold tracking-tight">{group.label.replace(/ Settings$/i, '')}</h2>
				<p class="text-sm text-muted-foreground">
					{group.items.length}
					{group.items.length === 1 ? 'setting' : 'settings'}. Changes take effect as soon as they are
					saved.
				</p>
			</div>
			{#if group.items.length > 4}
				<label class="relative block w-full max-w-xs">
					<SearchIcon
						class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
					/>
					<input
						type="search"
						placeholder="Filter settings"
						aria-label="Filter settings"
						class="h-9 w-full rounded-sm border border-input bg-card pr-3 pl-9 text-sm"
						bind:value={query}
					/>
				</label>
			{/if}
		</div>

		{#if group.items.length === 0}
			<p class="rounded-md border border-dashed border-border bg-card p-8 text-center text-sm text-muted-foreground">
				No settings in this group yet.
			</p>
		{:else if items.length === 0}
			<p class="text-sm text-muted-foreground">Nothing matches “{query}”.</p>
		{:else}
			<div class="divide-y divide-border rounded-md border border-border bg-card shadow-xs">
				{#each items as item (item.key)}
					<SettingField {item} onsaved={onSaved} />
				{/each}
			</div>
		{/if}
	</section>
{/if}

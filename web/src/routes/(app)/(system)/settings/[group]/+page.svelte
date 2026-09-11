<script lang="ts">
	import { page } from '$app/stores';
	import SettingField from '$lib/components/pages/settings/SettingField.svelte';
	import { getContext } from 'svelte';
	import type { CatalogGroup, CatalogItem } from '$lib/settings/api';

	type SettingsCtx = {
		groups: CatalogGroup[];
		setGroups: (groups: CatalogGroup[]) => void;
	};

	const ctx = getContext<SettingsCtx>('settings-groups');

	const groupSlug = $derived(
		($page.params as Record<string, string | undefined>).group ??
			($page.params as Record<string, string | undefined>).slug
	);

	const group = $derived(ctx?.groups?.find((g) => g.slug === groupSlug) ?? null);

	function onSaved(updated: CatalogItem) {
		if (!ctx || !group) return;
		const next = ctx.groups.map((g) =>
			g.slug !== group.slug
				? g
				: {
						...g,
						items: g.items.map((item) =>
							item.key === updated.key ? { ...item, ...updated } : item
						)
					}
		);
		ctx.setGroups(next);
	}
</script>

{#if !group}
	<div class="p-8">
		<h1 class="text-2xl font-semibold tracking-tight">Settings</h1>
		<p class="mt-2 text-sm text-muted-foreground">
			No settings group named <code class="text-xs">{groupSlug}</code> was found.
		</p>
	</div>
{:else}
	<div class="mx-auto max-w-3xl p-8">
		<header class="mb-2 space-y-2 border-b border-border pb-6">
			<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">
				Settings
			</p>
			<h1 class="text-3xl font-semibold tracking-tight">{group.label}</h1>
			<p class="text-sm text-muted-foreground">
				Effective values (org overrides win when present). Changes save per field.
			</p>
		</header>

		{#if group.items.length === 0}
			<p class="py-8 text-sm text-muted-foreground">No settings in this group yet.</p>
		{:else}
			{#each group.items as item (item.key)}
				<SettingField {item} onsaved={onSaved} />
			{/each}
		{/if}
	</div>
{/if}

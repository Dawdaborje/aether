<script lang="ts">
	import SettingSideBar from '$lib/components/pages/layout/settingSideBar.svelte';
	import { fetchSettingsCatalog, type CatalogGroup } from '$lib/settings/api';
	import { onMount } from 'svelte';
	import { setContext } from 'svelte';

	let { children } = $props();

	let groups = $state<CatalogGroup[]>([]);
	let loading = $state(true);
	let error = $state('');

	setContext('settings-groups', {
		get groups() {
			return groups;
		},
		setGroups(next: CatalogGroup[]) {
			groups = next;
		}
	});

	onMount(async () => {
		try {
			const catalog = await fetchSettingsCatalog();
			groups = catalog.groups;
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load settings';
		} finally {
			loading = false;
		}
	});
</script>

<div class="grid min-h-[calc(100vh-3.5rem)] w-full grid-cols-[auto_1fr] items-stretch">
	<SettingSideBar {groups} />
	<div class="min-w-0 overflow-auto bg-background">
		{#if loading}
			<div class="p-8 text-sm text-muted-foreground">Loading settings…</div>
		{:else if error}
			<div class="p-8 text-sm text-destructive">{error}</div>
		{:else}
			{@render children()}
		{/if}
	</div>
</div>

<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import SettingsNav from '$lib/components/settings/SettingsNav.svelte';
	import { fetchSettingsCatalog, type CatalogGroup } from '$lib/settings/api';
	import { onMount, setContext } from 'svelte';

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
			groups = (await fetchSettingsCatalog()).groups;
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to load settings';
		} finally {
			loading = false;
		}
	});
</script>

<ControlPanel crumbs={[{ label: 'Settings' }]} />
<div class="w-full space-y-4 px-4 py-4 md:px-6">

	<div class="grid gap-6 md:grid-cols-[12rem_minmax(0,1fr)]">
		<SettingsNav {groups} />
		<div class="min-w-0">
			{#if loading}
				<p class="text-sm text-muted-foreground">Loading settings…</p>
			{:else if error}
				<p class="text-sm text-destructive" role="alert">{error}</p>
			{:else}
				{@render children()}
			{/if}
		</div>
	</div>
</div>

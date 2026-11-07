<script lang="ts">
	import { DetailSkeleton } from '$lib/components/ui/skeleton';
	import { reportError } from '$lib/errors/report';
	import { page } from '$app/state';
	import { settingsCrumbs } from '$lib/components/layout/crumbs';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import SettingsNav from '$lib/components/settings/SettingsNav.svelte';
	import { fetchSettingsCatalog, type CatalogGroup } from '$lib/settings/api';
	import { onMount, setContext } from 'svelte';

	let { children } = $props();

	// General always comes first; the rest keep the order the server gave.
	const isGeneral = (group: CatalogGroup) =>
		group.slug === 'general' || /^general( settings)?$/i.test(group.label);
	const generalFirst = (list: CatalogGroup[]) =>
		[...list].sort((a, b) => Number(isGeneral(b)) - Number(isGeneral(a)));

	let groups = $state<CatalogGroup[]>([]);
	let loading = $state(true);
	let error = $state('');

	setContext('settings-groups', {
		get groups() {
			return groups;
		},
		setGroups(next: CatalogGroup[]) {
			groups = generalFirst(next);
		}
	});

	onMount(async () => {
		try {
			groups = generalFirst((await fetchSettingsCatalog()).groups);
		} catch (err) {
			error = reportError(err, 'Failed to load settings');
		} finally {
			loading = false;
		}
	});
</script>

<ControlPanel crumbs={settingsCrumbs(page.url.pathname, groups)} />
<div class="w-full space-y-4 px-4 py-4 md:px-6">
	<div class="grid gap-6 md:grid-cols-[12rem_minmax(0,1fr)]">
		<SettingsNav {groups} />
		<div class="min-w-0">
			{#if loading}
				<DetailSkeleton fields={6} label="Loading settings…" />
			{:else if error}
				<p class="text-sm text-destructive" role="alert">{error}</p>
			{:else}
				{@render children()}
			{/if}
		</div>
	</div>
</div>

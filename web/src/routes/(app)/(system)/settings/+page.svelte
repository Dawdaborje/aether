<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getContext } from 'svelte';
	import type { CatalogGroup } from '$lib/settings/api';

	type SettingsCtx = { groups: CatalogGroup[] };
	const ctx = getContext<SettingsCtx>('settings-groups');

	// Open the first group; with none, fall through to Appearance.
	$effect(() => {
		const first = ctx?.groups?.[0]?.slug;
		void goto(resolve(first ? `/settings/${first}` : '/settings/appearance'), { replaceState: true });
	});
</script>

<p class="text-sm text-muted-foreground">Opening settings…</p>

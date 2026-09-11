<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { getContext } from 'svelte';
	import type { CatalogGroup } from '$lib/settings/api';

	type SettingsCtx = {
		groups: CatalogGroup[];
	};

	const ctx = getContext<SettingsCtx>('settings-groups');

	$effect(() => {
		const first = ctx?.groups?.[0]?.slug;
		if (first) {
			goto(resolve(`/settings/${first}`), { replaceState: true });
		}
	});
</script>

<div class="p-8 text-sm text-muted-foreground">Opening settings…</div>

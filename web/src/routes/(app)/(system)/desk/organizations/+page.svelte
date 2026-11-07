<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { errorMessage } from '$lib/errors/report';
	import { fetchOrganizations } from '$lib/org/api';

	// An installation has one organization, so there is no list to show: go to it.
	let failed = $state<string | null>(null);

	async function open() {
		failed = null;
		try {
			const [only] = await fetchOrganizations();
			if (only) {
				await goto(resolve(`/desk/organizations/${only.db_name}/overview`), { replaceState: true });
			} else {
				failed = 'This installation has no organization yet. Run `aether --init`.';
			}
		} catch (error) {
			failed = errorMessage(error, 'The organization could not be loaded.');
		}
	}

	onMount(open);
</script>

{#if failed}
	<LoadFailed message={failed} onRetry={open} retrying={false} />
{/if}

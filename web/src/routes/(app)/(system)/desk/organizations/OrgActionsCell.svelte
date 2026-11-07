<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import type { OrganizationProfile } from '$lib/org/api';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { org }: { org: OrganizationProfile } = $props();
</script>

<!-- The row opens the organization on click; these buttons must not also trigger that. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="flex items-center justify-end gap-2"
	onclick={(event) => event.stopPropagation()}
	onkeydown={(event) => event.stopPropagation()}
>
	<Button variant="outline" size="sm" href={resolve(`/desk/organizations/${org.db_name}/overview`)}>
		Open
	</Button>
	{#if org.db_name !== orgStore.current && orgStore.mode !== 'address' && !org.disabled_at}
		<Button
			variant="secondary"
			size="sm"
			disabled={orgStore.switching}
			onclick={async () => {
				await orgStore.choose(org.db_name);
				if (!orgStore.error) await goto(resolve('/apps'));
			}}>Enter</Button
		>
	{/if}
</div>

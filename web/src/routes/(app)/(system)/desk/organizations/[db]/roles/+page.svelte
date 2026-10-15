<script lang="ts">
	import RolesManager from '$lib/components/org/RolesManager.svelte';

	let { params }: { params: { db: string } } = $props();
</script>

<RolesManager db={params.db} title="Roles" detailMode="desk" />

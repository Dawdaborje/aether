<script lang="ts">
	import MembersManager from '$lib/components/org/MembersManager.svelte';

	let { params }: { params: { db: string } } = $props();
</script>

<MembersManager db={params.db} title="Members" />

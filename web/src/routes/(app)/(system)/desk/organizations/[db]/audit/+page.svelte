<script lang="ts">
	import AuditPanel from '$lib/components/audit/AuditPanel.svelte';

	let { params }: { params: { db: string } } = $props();
</script>

{#key params.db}
	<AuditPanel db={params.db} />
{/key}

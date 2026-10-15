<script lang="ts">
	import SchedulesPanel from '$lib/components/schedules/SchedulesPanel.svelte';

	let { params }: { params: { db: string } } = $props();
</script>

<div class="space-y-3 py-2">
	<p class="text-sm text-muted-foreground">
		Recurring tasks from plugins installed on this organization, and their run history.
	</p>
	{#key params.db}
		<SchedulesPanel db={params.db} />
	{/key}
</div>

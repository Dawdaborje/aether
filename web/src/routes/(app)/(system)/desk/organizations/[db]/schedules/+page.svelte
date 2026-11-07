<script lang="ts">
	import SchedulesPanel from '$lib/components/schedules/SchedulesPanel.svelte';

	let { params }: { params: { db: string } } = $props();
</script>

<div class="flex min-h-0 w-full flex-1 flex-col gap-3">
	<p class="text-sm text-muted-foreground">
		Recurring tasks from plugins installed on this organization, and their run history.
	</p>
	{#key params.db}
		<SchedulesPanel db={params.db} />
	{/key}
</div>

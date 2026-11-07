<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';
	import { scheduleTitle, type ScheduleTask } from '$lib/schedules/api';

	let { task }: { task: ScheduleTask } = $props();
</script>

<div class="flex flex-wrap items-center gap-2">
	<span class="font-medium">{scheduleTitle(task)}</span>
	<Badge variant="outline" class="font-mono text-[11px] font-normal">{task.plugin}</Badge>
</div>
{#if task.description}
	<div class="line-clamp-2 max-w-xl text-xs text-muted-foreground">{task.description}</div>
{/if}

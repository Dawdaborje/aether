<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import type { ScheduleTask } from '$lib/schedules/api';

	let {
		task,
		busy = false,
		onStop,
		onRestart,
		onRunNow
	}: {
		task: ScheduleTask;
		busy?: boolean;
		onStop: (task: ScheduleTask, event: MouseEvent) => void;
		onRestart: (task: ScheduleTask, event: MouseEvent) => void;
		onRunNow: (task: ScheduleTask, event: MouseEvent) => void;
	} = $props();
</script>

<div class="flex justify-end gap-1">
	{#if task.enabled}
		<Button size="xs" variant="outline" disabled={busy} onclick={(e) => onStop(task, e)}>
			Stop
		</Button>
	{:else}
		<Button size="xs" variant="outline" disabled={busy} onclick={(e) => onRestart(task, e)}>
			Restart
		</Button>
	{/if}
	<Button size="xs" disabled={busy} onclick={(e) => onRunNow(task, e)}>Run</Button>
</div>

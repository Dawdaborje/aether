<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import HealthPanel from '$lib/components/health/HealthPanel.svelte';
</script>

<ControlPanel crumbs={[{ label: 'System health' }]} />

<div class="flex w-full flex-1 flex-col gap-4 px-4 py-4 md:px-6">
	<HealthPanel />
</div>

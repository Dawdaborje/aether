<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import MantlePanel from '$lib/components/mantle/MantlePanel.svelte';
</script>

<ControlPanel crumbs={[{ label: 'Mantle' }]} />

<div class="flex w-full flex-1 flex-col gap-4 px-4 py-4 md:px-6">
	<MantlePanel />
</div>

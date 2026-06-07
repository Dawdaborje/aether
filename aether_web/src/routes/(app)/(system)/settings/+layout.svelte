<script>
	import SettingSideBar from '$lib/components/pages/layout/settingSideBar.svelte';

	let { children } = $props();
</script>

<div class="grid h-screen w-full grid-cols-[auto_1fr] items-stretch border border-surface-200-800">
	<SettingSideBar />
	{@render children()}
</div>

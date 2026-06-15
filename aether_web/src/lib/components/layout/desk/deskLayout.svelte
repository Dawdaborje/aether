<script>
	import DeskSideBar from './deskSideBar.svelte';

	let { children } = $props();
</script>

<div class="grid h-screen w-full grid-cols-[auto_1fr] items-stretch border border-surface-200-800">
	<DeskSideBar />
	<div class="flex items-center justify-center">
		{@render children()}
	</div>
</div>

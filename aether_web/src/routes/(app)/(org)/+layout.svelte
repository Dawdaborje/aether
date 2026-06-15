<script>
	import DeskLayout from '$lib/components/layout/desk/deskLayout.svelte';

	let { children } = $props();
</script>

<DeskLayout>
	{@render children()}
</DeskLayout>

<script>
	import DefaultNavbar from './defaultNavbar.svelte';

	let { children } = $props();
</script>

<div class="flex flex-col">
	<DefaultNavbar />
	{@render children()}
</div>

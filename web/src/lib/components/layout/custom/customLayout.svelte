<script lang="ts">
	import CustomNavbar from './customNavbar.svelte';

	let { children } = $props();
</script>

<div class="flex min-h-screen w-full flex-col">
	<CustomNavbar />
	<main class="flex-1 p-6">
		{@render children()}
	</main>
</div>

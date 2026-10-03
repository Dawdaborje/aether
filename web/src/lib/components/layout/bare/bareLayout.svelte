<script>
	// No application chrome: used for public pages and for pages that ask for it.
	let { children } = $props();
</script>

<main class="min-h-screen">
	{@render children()}
</main>

<script>
	// No application chrome: used for public pages and for pages that ask for it.
	let { children } = $props();
</script>

<!-- Exactly one viewport tall: a page's own sheet scrolls inside it, and anything else scrolls here. -->
<main class="flex h-dvh min-h-0 flex-col overflow-y-auto">
	{@render children()}
</main>

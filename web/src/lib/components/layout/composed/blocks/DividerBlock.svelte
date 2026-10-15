<script lang="ts">
	let { vertical = false }: { vertical?: boolean } = $props();
</script>

<!-- In a row (top bar) a short vertical rule; in a column (sidebar) a horizontal one. -->
<div
	role="separator"
	class={vertical ? 'my-2 h-px w-full bg-region-border' : 'mx-1 h-5 w-px bg-region-border'}
></div>

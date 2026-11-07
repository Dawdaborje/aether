<script lang="ts">
	/** A miniature of the app drawn in a theme's own colours. */
	let {
		background,
		foreground,
		primary,
		sidebar
	}: { background: string; foreground: string; primary: string; sidebar: string } = $props();
</script>

<div
	class="flex h-24 overflow-hidden rounded-sm border border-border"
	style={`background: ${background}`}
	aria-hidden="true"
>
	<div class="w-1/4" style={`background: ${sidebar}`}></div>
	<div class="flex flex-1 flex-col gap-2 p-2.5">
		<div class="h-2 w-1/3 rounded-full" style={`background: ${foreground}; opacity: 0.75`}></div>
		<div class="h-1.5 w-3/4 rounded-full" style={`background: ${foreground}; opacity: 0.2`}></div>
		<div class="h-1.5 w-2/3 rounded-full" style={`background: ${foreground}; opacity: 0.2`}></div>
		<div class="mt-auto h-3.5 w-1/3 rounded-sm" style={`background: ${primary}`}></div>
	</div>
</div>

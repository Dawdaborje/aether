<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { cn } from '$lib/utils.js';

	let {
		label = 'Loading…',
		class: className
	}: {
		label?: string;
		class?: string;
	} = $props();
</script>

<div
	role="status"
	aria-live="polite"
	class={cn(
		'flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground',
		className
	)}
>
	<AppIcon name="in-progress" class="size-4 animate-spin" aria-hidden="true" />
	<span>{label}</span>
</div>

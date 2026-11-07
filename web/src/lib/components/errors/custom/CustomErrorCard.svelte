<script lang="ts">
	import type { ErrorCardProps } from '../types';

	let { title, message }: ErrorCardProps = $props();
</script>

<div
	role="alert"
	class="rounded-r-md border-l-4 border-primary bg-muted/50 p-4 text-card-foreground"
>
	<p class="text-sm font-semibold text-primary">{title}</p>
	{#if message}
		<p class="mt-1 text-sm text-muted-foreground">{message}</p>
	{/if}
</div>

<script lang="ts">
	import type { ErrorCardProps } from '../types';

	let { title, message }: ErrorCardProps = $props();
</script>

<div role="alert" class="rounded-md border border-destructive/40 bg-card p-4 text-card-foreground">
	<p class="text-sm font-semibold text-destructive">{title}</p>
	{#if message}
		<p class="mt-1 text-sm text-muted-foreground">{message}</p>
	{/if}
</div>

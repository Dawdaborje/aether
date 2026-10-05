<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';
	import type { SaveResult } from './api';

	let { result, ondismiss }: { result: SaveResult; ondismiss: () => void } = $props();
</script>

<div class="relative mb-4 space-y-1 rounded-md border border-primary/30 bg-primary/5 p-3 pr-10 text-sm" role="status">
	<button type="button" class="absolute top-2 right-2 text-muted-foreground hover:text-foreground" aria-label="Dismiss" onclick={ondismiss}>
		<XIcon class="size-4" />
	</button>
	<p class="font-medium">Saved as version {result.version}</p>
	<p class="text-muted-foreground">
		{result.file.written ? `Model file written: ${result.file.path}` : `Model file not written (${result.file.reason}).`}
	</p>
	{#each result.applied as applied (applied.organization)}
		<p class={applied.ok ? 'text-muted-foreground' : 'text-destructive'}>
			{applied.organization}: {applied.ok ? 'moved to the new version' : applied.error}
		</p>
	{/each}
	<p class="text-xs text-muted-foreground">{result.note}</p>
</div>

<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { downloadMedia, formatSize } from '../api';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let error = $state<string | null>(null);

	async function save() {
		error = null;
		try {
			await downloadMedia(ctx.file);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The download failed.';
		}
	}
</script>

<div class="space-y-2 p-6 text-sm text-muted-foreground">
	<p>This kind of file cannot be previewed.</p>
	<p class="text-xs">
		{ctx.file.name} · {formatSize(ctx.file.size)}{ctx.file.mime ? ` · ${ctx.file.mime}` : ''}
	</p>
	{#if error}<p class="text-destructive" role="alert">{error}</p>{/if}
	<Button size="sm" onclick={() => void save()}>Download {ctx.file.name}</Button>
</div>

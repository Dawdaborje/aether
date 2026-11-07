<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let url = $state('');
	let error = $state<string | null>(null);
	let zoom = $state(1);
	let turn = $state(0);

	$effect(() => {
		const current = ctx;
		url = '';
		error = null;
		zoom = 1;
		turn = 0;
		current.setPrint(() => window.print());
		current
			.objectUrl()
			.then((u) => (url = u))
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'));
		return () => current.setPrint(null);
	});
</script>

<div class="flex h-full flex-col">
	<div class="flex shrink-0 items-center gap-1 border-b border-border bg-background px-2 py-1">
		<Button
			size="icon-xs"
			variant="ghost"
			aria-label="Zoom out"
			onclick={() => (zoom = Math.max(0.25, zoom - 0.25))}><AppIcon name="zoom-out" /></Button
		>
		<span class="w-10 text-center text-xs text-muted-foreground">{Math.round(zoom * 100)}%</span>
		<Button
			size="icon-xs"
			variant="ghost"
			aria-label="Zoom in"
			onclick={() => (zoom = Math.min(4, zoom + 0.25))}><AppIcon name="zoom-in" /></Button
		>
		<Button size="icon-xs" variant="ghost" aria-label="Fit to window" onclick={() => (zoom = 1)}
			><AppIcon name="maximize" /></Button
		>
		<Button
			size="icon-xs"
			variant="ghost"
			aria-label="Rotate"
			onclick={() => (turn = (turn + 90) % 360)}><AppIcon name="rotate-clockwise" /></Button
		>
	</div>
	<div class="min-h-0 flex-1 overflow-auto">
		{#if error}
			<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
		{:else if !url}
			<p class="p-4 text-sm text-muted-foreground">Opening…</p>
		{:else}
			<div class="flex min-h-full items-center justify-center p-2">
				<img
					src={url}
					alt={ctx.file.name}
					style="width:{zoom * 100}%;max-width:none;height:auto;transform:rotate({turn}deg)"
					class="object-contain"
				/>
			</div>
		{/if}
	</div>
</div>

<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Sheet from '$lib/components/ui/sheet';
	import { Button } from '$lib/components/ui/button';
	import { formatSize, viewHref, type MediaFile } from './api';
	import ViewerPane from './ViewerPane.svelte';

	/**
	 * A side sheet that shows one file of a list and steps through the others, so a person can go
	 * through a folder or a record's attachments without leaving the page. With `mode="modal"` it is a near
	 * full-screen dialog instead, for documents that need room (PDFs, spreadsheets, long text).
	 */
	let {
		files,
		index = $bindable(0),
		open = $bindable(false),
		mode = 'sheet'
	}: { files: MediaFile[]; index?: number; open?: boolean; mode?: 'sheet' | 'modal' } = $props();

	const file = $derived(files[Math.min(Math.max(0, index), files.length - 1)] ?? null);

	function onkeydown(event: KeyboardEvent) {
		if (!open || (event.target as HTMLElement)?.closest('input, textarea')) return;
		if (event.key === 'ArrowLeft' && index > 0) index--;
		else if (event.key === 'ArrowRight' && index < files.length - 1) index++;
	}
</script>

<svelte:window {onkeydown} />

{#snippet details()}
	{file ? formatSize(file.size) : ''}{file?.mime ? ` · ${file.mime}` : ''}
	{files.length > 1 ? ` · ${index + 1} of ${files.length}` : ''}
{/snippet}

{#snippet controls()}
	<div class="flex shrink-0 items-center gap-1 px-4">
		<Button
			size="icon-xs"
			variant="ghost"
			aria-label="Previous file"
			disabled={index <= 0}
			onclick={() => index--}><AppIcon name="chevron-left" /></Button
		>
		<Button
			size="icon-xs"
			variant="ghost"
			aria-label="Next file"
			disabled={index >= files.length - 1}
			onclick={() => index++}><AppIcon name="chevron-right" /></Button
		>
		{#if file}
			<a
				href={viewHref(file)}
				class="ml-auto inline-flex items-center gap-1 text-xs text-muted-foreground hover:underline"
				><AppIcon name="maximize" class="size-3" /> Open full page</a
			>
		{/if}
	</div>
	<div class="min-h-0 flex-1 px-4 pb-4">
		{#if open && file}{#key file.key}<ViewerPane {file} />{/key}{/if}
	</div>
{/snippet}

{#if mode === 'modal'}
	<Dialog.Root bind:open>
		<Dialog.Content
			class="flex h-[94vh] w-[96vw] max-w-none flex-col gap-3 sm:h-[94vh] sm:w-[96vw] sm:max-w-none"
			data-part="preview-modal"
		>
			<Dialog.Header class="shrink-0 pr-10">
				<Dialog.Title class="truncate">{file?.name ?? 'File'}</Dialog.Title>
				<Dialog.Description>{@render details()}</Dialog.Description>
			</Dialog.Header>
			{@render controls()}
		</Dialog.Content>
	</Dialog.Root>
{:else}
	<Sheet.Root bind:open>
		<Sheet.Content side="right" class="flex w-full flex-col gap-3 sm:max-w-3xl">
			<Sheet.Header class="shrink-0 pr-10">
				<Sheet.Title class="truncate">{file?.name ?? 'File'}</Sheet.Title>
				<Sheet.Description>{@render details()}</Sheet.Description>
			</Sheet.Header>
			{@render controls()}
		</Sheet.Content>
	</Sheet.Root>
{/if}

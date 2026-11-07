<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { formatSize, type MediaFile } from './api';
	import ViewerPane from './ViewerPane.svelte';

	/**
	 * Opens a stored file by its key in a dialog. The bytes come through the server, so it does not
	 * matter which storage the organization uses (disk, S3-compatible, or another backend). Which viewer
	 * shows the file is decided by the registry in `viewers/`.
	 */
	let { file, open = $bindable(false) }: { file: MediaFile | null; open?: boolean } = $props();
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="flex h-[85vh] max-w-5xl flex-col gap-3 sm:max-w-5xl">
		<Dialog.Header class="shrink-0 pr-10">
			<Dialog.Title class="truncate">{file?.name ?? 'File'}</Dialog.Title>
			<Dialog.Description>
				{file ? formatSize(file.size) : ''}{file?.mime ? ` · ${file.mime}` : ''}
			</Dialog.Description>
		</Dialog.Header>
		<div class="min-h-0 flex-1">
			{#if open && file}
				<ViewerPane {file} />
			{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>

<script lang="ts">
	import { page } from '$app/state';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { formatSize, isMediaFile, type MediaFile } from '$lib/media/api';
	import ViewerPane from '$lib/media/ViewerPane.svelte';

	// `/view/<media key>?name=…&mime=…&size=…`: one file, full screen. The server still decides who may read it.
	const file = $derived.by((): MediaFile | null => {
		const key = page.params.key ?? '';
		const q = page.url.searchParams;
		const candidate = {
			key,
			name: q.get('name') || key.split('/').pop() || 'file',
			mime: q.get('mime') ?? '',
			size: Number(q.get('size')) || 0
		};
		return isMediaFile(candidate) ? candidate : null;
	});
</script>

<ControlPanel crumbs={[{ label: file?.name ?? 'File' }]} />

<div class="h-[calc(100dvh-9rem)] w-full px-4 py-4 md:px-6">
	{#if file}
		<p class="mb-2 text-xs text-muted-foreground">
			{file.size ? formatSize(file.size) : ''}{file.mime ? ` · ${file.mime}` : ''}
		</p>
		<div class="h-[calc(100%-1.5rem)]"><ViewerPane {file} /></div>
	{:else}
		<p class="text-sm text-muted-foreground">There is no such file.</p>
	{/if}
</div>

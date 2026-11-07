<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { isMediaFile, type MediaFile } from '$lib/media/api';
	import ViewerPane from '$lib/media/ViewerPane.svelte';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';

	/**
	 * `<viewer source="get_file" height="480" />` shows a file inline with the universal viewer.
	 * `source` is a function of the plugin, called with the page's route values, that returns a file
	 * (`{ key, name, mime, size }`) or `{ file }`. Without `source`, `key` and `name` give the file.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const height = $derived(Math.min(1200, Math.max(120, Number(node.height) || 480)));

	let file = $state<MediaFile | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);
	let latest = 0;

	function fromNode(): MediaFile | null {
		const key = text(node.key);
		if (!key) return null;
		const candidate = {
			key,
			name: text(node.name) ?? key.split('/').pop() ?? 'file',
			mime: text(node.mime) ?? '',
			size: Number(node.size) || 0
		};
		return isMediaFile(candidate) ? candidate : null;
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		const fn = source;
		if (!fn) {
			file = fromNode();
			error = file ? null : 'This viewer has no file.';
			return;
		}
		if (!page) return;
		const current = ++latest;
		loading = true;
		void page.call(fn, page.args).then((result) => {
			if (current !== latest) return;
			loading = false;
			if (!result.ok) {
				error = result.error;
				return;
			}
			const data = result.data as { file?: unknown } | null;
			const found = isMediaFile(result.data)
				? result.data
				: isMediaFile(data?.file)
					? data.file
					: null;
			file = found;
			error = found ? null : 'There is no file to show.';
		});
	});
</script>

<div class="my-2 overflow-hidden rounded-md" style="height:{height}px" data-part="viewer">
	{#if loading && !file}
		<p class="p-4 text-sm text-muted-foreground">Opening…</p>
	{:else if error && !file}
		<p class="p-4 text-sm text-muted-foreground">{error}</p>
	{:else if file}
		<ViewerPane {file} />
	{/if}
</div>

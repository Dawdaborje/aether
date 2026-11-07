<script lang="ts">
	import { formatSize } from '../api';
	import { listArchive, type ArchiveEntry } from '../officePreview';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let entries = $state<ArchiveEntry[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		current
			.bytes()
			.then((b) => listArchive(b))
			.then((list) => (entries = list))
			.catch(
				() =>
					(error = 'The contents of this archive cannot be listed here. Download it to open it.')
			)
			.finally(() => (loading = false));
	});
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-muted-foreground">{error}</p>
{:else}
	<table class="w-full border-collapse text-xs">
		<thead>
			<tr class="bg-muted text-left"
				><th class="px-2 py-1">Name</th><th class="px-2 py-1 text-right">Size</th></tr
			>
		</thead>
		<tbody>
			{#each entries as entry (entry.name)}
				<tr class="border-t border-border">
					<td class="px-2 py-1 break-all">{entry.name}</td>
					<td class="px-2 py-1 text-right whitespace-nowrap"
						>{entry.name.endsWith('/') ? '' : formatSize(entry.size)}</td
					>
				</tr>
			{/each}
		</tbody>
	</table>
	<p class="p-2 text-xs text-muted-foreground">
		{entries.length} entries. Nothing is unpacked here.
	</p>
{/if}

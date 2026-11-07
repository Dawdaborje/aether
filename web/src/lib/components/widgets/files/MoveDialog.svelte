<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { getPageState } from '$lib/pages/pageContext.svelte';

	/** Choose where to move something: the folders the person may write to (from the `tree` function), or the top. */
	let {
		open = $bindable(false),
		tree,
		exclude = '',
		onsubmit
	}: {
		open?: boolean;
		tree?: string;
		/** A folder that cannot be chosen (the one being moved). */
		exclude?: string;
		onsubmit: (to: string) => Promise<string | null | void>;
	} = $props();

	const page = getPageState();
	let folders = $state<{ id: string; name: string }[]>([]);
	let chosen = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (!open || !tree || !page) return;
		chosen = '';
		error = null;
		void page.call(tree, {}).then((r) => {
			folders = r.ok && Array.isArray(r.data) ? (r.data as { id: string; name: string }[]) : [];
		});
	});

	async function submit() {
		busy = true;
		error = (await onsubmit(chosen)) ?? null;
		busy = false;
		if (!error) open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header><Dialog.Title>Move to</Dialog.Title></Dialog.Header>
		<div class="max-h-72 space-y-1 overflow-y-auto" role="radiogroup" aria-label="Folders">
			<label class="flex items-center gap-2 rounded px-2 py-1.5 text-sm hover:bg-muted">
				<input type="radio" name="dest" value="" bind:group={chosen} /> My files (top)
			</label>
			{#each folders.filter((f) => f.id !== exclude) as folder (folder.id)}
				<label class="flex items-center gap-2 rounded px-2 py-1.5 text-sm hover:bg-muted">
					<input type="radio" name="dest" value={folder.id} bind:group={chosen} />
					<span class="truncate">{folder.name}</span>
				</label>
			{/each}
		</div>
		{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
		<Dialog.Footer>
			<Button type="button" variant="ghost" onclick={() => (open = false)}>Cancel</Button>
			<Button type="button" disabled={busy} onclick={() => void submit()}>Move</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

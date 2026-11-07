<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { ago } from '$lib/pages/files';
	import { getPageState } from '$lib/pages/pageContext.svelte';

	/**
	 * The comments on one file. `comments` lists them (`[{ id, author_name, body, at, can_delete }]`, called with
	 * `{ file }`), `comment` adds one (`{ file, body }`) and `remove` deletes one (`{ id }`).
	 */
	let {
		open = $bindable(false),
		file,
		name,
		comments,
		comment,
		remove
	}: {
		open?: boolean;
		file: string;
		name: string;
		comments: string;
		comment?: string;
		remove?: string;
	} = $props();

	type Row = { id: string; author_name: string; body: string; at?: string; can_delete?: boolean };
	const page = getPageState();
	let rows = $state<Row[]>([]);
	let body = $state('');
	let error = $state<string | null>(null);
	let busy = $state(false);

	async function load() {
		if (!page) return;
		const r = await page.call(comments, { file });
		if (r.ok && Array.isArray(r.data)) rows = r.data as Row[];
		else if (!r.ok) error = r.error;
	}

	$effect(() => {
		if (!open) return;
		error = null;
		body = '';
		void load();
	});

	async function add() {
		if (!comment || !page || !body.trim()) return;
		busy = true;
		const r = await page.call(comment, { file, body });
		busy = false;
		if (!r.ok) {
			error = r.error;
			return;
		}
		error = null;
		body = '';
		await load();
	}

	async function drop(id: string) {
		if (!remove || !page) return;
		const r = await page.call(remove, { id });
		if (!r.ok) error = r.error;
		await load();
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>Comments on “{name}”</Dialog.Title>
		</Dialog.Header>
		<ul
			class="max-h-72 divide-y divide-border overflow-y-auto rounded-md border border-border"
			data-part="comments"
		>
			{#each rows as row (row.id)}
				<li class="space-y-0.5 px-3 py-2 text-sm">
					<div class="flex items-center gap-2 text-xs text-muted-foreground">
						<span class="font-medium text-foreground">{row.author_name}</span>
						<span>{ago(row.at)}</span>
						{#if row.can_delete && remove}
							<button
								type="button"
								class="ml-auto hover:text-foreground"
								aria-label="Delete this comment"
								onclick={() => void drop(row.id)}><AppIcon name="trash" class="size-3.5" /></button
							>
						{/if}
					</div>
					<p class="break-words whitespace-pre-wrap">{row.body}</p>
				</li>
			{:else}
				<li class="px-3 py-3 text-sm text-muted-foreground">No comments yet.</li>
			{/each}
		</ul>
		{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
		{#if comment}
			<form
				class="flex gap-2"
				onsubmit={(e) => {
					e.preventDefault();
					void add();
				}}
			>
				<textarea
					bind:value={body}
					rows="2"
					aria-label="Write a comment"
					placeholder="Write a comment"
					class="min-w-0 flex-1 rounded-md border border-input bg-transparent px-2 py-1.5 text-sm"
				></textarea>
				<Button type="submit" disabled={!body.trim() || busy}>Comment</Button>
			</form>
		{/if}
		<Dialog.Footer>
			<Button type="button" variant="outline" onclick={() => (open = false)}>Close</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

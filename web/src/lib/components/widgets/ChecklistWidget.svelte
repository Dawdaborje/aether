<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { useLiveTopic } from '$lib/pages/liveTopic.svelte';
	import { getPageState, keyOf, rowsOf, text } from '$lib/pages/pageContext.svelte';

	/**
	 * `<checklist source="task_checklist" add="add_check" toggle="toggle_check" remove="remove_check"/>`: a list of things
	 * to tick off. `source` is called with the page's values and answers rows `{ id, text, done }`. `add` is called with
	 * `{ text }`, `toggle` with `{ id, done }` and `remove` with `{ id }` (plus the page's values); a function that is not
	 * named is not offered. The count of what is done is shown above the list.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const live = useLiveTopic(() => node.topic);
	const source = $derived(text(node.source));
	const fn = (name: string) => text((node as Record<string, unknown>)[name]);

	interface Check {
		id: string;
		text: string;
		done: boolean;
	}
	let items = $state<Check[]>([]);
	let loaded = $state(false);
	let draft = $state('');
	let error = $state<string | null>(null);
	let latest = 0;

	async function load() {
		if (!source || !page) return;
		const current = ++latest;
		const result = await page.call(source, page.args);
		if (current !== latest) return;
		loaded = true;
		if (!result.ok) {
			error = result.error;
			return;
		}
		error = null;
		items = (rowsOf(result.data) as Record<string, unknown>[]).map((r) => ({
			id: keyOf(r.id),
			text: String(r.text ?? ''),
			done: r.done === true
		}));
	}

	$effect(() => {
		void page?.plugin;
		void JSON.stringify(page?.args ?? {});
		void page?.tick;
		void live.n;
		untrack(() => void load());
	});

	async function call(name: string | undefined, payload: Record<string, unknown>) {
		if (!name || !page) return;
		const r = await page.call(name, { ...page.args, ...payload });
		if (!r.ok) error = r.error;
		await load();
	}

	async function add() {
		const value = draft.trim();
		if (!value) return;
		draft = '';
		await call(fn('add'), { text: value });
	}

	const done = $derived(items.filter((i) => i.done).length);
</script>

<section
	class="space-y-2"
	data-part="checklist"
	aria-label={text(node.title) ?? text(node.label) ?? 'Checklist'}
>
	<div class="flex items-center gap-2 text-sm">
		<h3 class="font-medium">{text(node.title) ?? text(node.label) ?? 'Checklist'}</h3>
		{#if items.length > 0}<span class="text-xs text-muted-foreground" data-part="checklist-count"
				>{done}/{items.length}</span
			>{/if}
	</div>
	{#if items.length > 0}
		<div class="h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden="true">
			<div
				class="h-full bg-primary transition-all"
				style="width:{(done / items.length) * 100}%"
			></div>
		</div>
	{/if}
	{#if error}<p class="text-xs text-destructive" role="alert">{error}</p>{/if}
	<ul class="space-y-1">
		{#each items as item (item.id)}
			<li
				class="group flex items-center gap-2 rounded px-1 py-0.5 hover:bg-muted/50"
				data-part="check-item"
			>
				<input
					type="checkbox"
					checked={item.done}
					disabled={!fn('toggle')}
					aria-label={item.text}
					onchange={(e) => void call(fn('toggle'), { id: item.id, done: e.currentTarget.checked })}
				/>
				<span
					class="min-w-0 flex-1 text-sm break-words {item.done
						? 'text-muted-foreground line-through'
						: ''}">{item.text}</span
				>
				{#if fn('remove')}
					<button
						type="button"
						class="text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-foreground focus-visible:opacity-100"
						aria-label="Remove {item.text}"
						onclick={() => void call(fn('remove'), { id: item.id })}
						><AppIcon name="close" class="size-3.5" /></button
					>
				{/if}
			</li>
		{/each}
	</ul>
	{#if loaded && items.length === 0}<p class="text-xs text-muted-foreground">
			Nothing to tick off yet.
		</p>{/if}
	{#if fn('add')}
		<form
			class="flex gap-2"
			onsubmit={(e) => {
				e.preventDefault();
				void add();
			}}
		>
			<input
				bind:value={draft}
				placeholder="Add an item"
				aria-label="Add an item"
				maxlength="200"
				class="h-8 min-w-0 flex-1 rounded-md border border-input bg-transparent px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
			/>
			<button
				type="submit"
				class="inline-flex h-8 items-center gap-1 rounded-md border border-border px-2 text-sm hover:bg-muted"
				disabled={!draft.trim()}><AppIcon name="plus" class="size-3.5" /> Add</button
			>
		</form>
	{/if}
</section>

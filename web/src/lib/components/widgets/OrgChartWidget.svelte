<script lang="ts">
	import { goto } from '$app/navigation';
	import { navHref } from '$lib/components/layout/nav';
	import type { PageNode } from '$lib/dsl/types';
	import MediaImage from '$lib/media/MediaImage.svelte';
	import type { MediaFile } from '$lib/media/api';
	import { orgTree, initials, type ChartNode } from '$lib/pages/orgchart';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';

	/**
	 * `<orgchart source="org_chart" open="/people/{id}"/>`: reporting lines drawn as a tree of cards. `source` answers
	 * rows of `{ id, name, title, manager, photo? }` (`manager` is the id of the row above, empty at the top); a card
	 * opens `open`, and a card with people below it can be folded.
	 */
	let { node }: { node: PageNode } = $props();
	const page = getPageState();
	let roots = $state<ChartNode[]>([]);
	let failure = $state<string | null>(null);
	let loading = $state(true);
	let folded = $state<Record<string, boolean>>({});

	$effect(() => {
		const source = text(node.source);
		void JSON.stringify(page?.args ?? {});
		void page?.tick;
		if (!source || !page) return;
		void page.call(source, page.args).then((r) => {
			loading = false;
			if (!r.ok) {
				failure = r.error;
				return;
			}
			failure = null;
			roots = orgTree(r.data);
		});
	});

	function open(id: string) {
		const template = text(node.open);
		if (template) void goto(navHref(interpolate(template, { ...(page?.vars ?? {}), id })));
	}
</script>

{#snippet card(n: ChartNode)}
	<div class="flex flex-col items-center" data-part="org-node">
		<div
			class="flex w-44 items-center gap-2 rounded-lg border border-border bg-background p-2 shadow-sm"
		>
			<button
				type="button"
				class="flex min-w-0 flex-1 items-center gap-2 text-left"
				onclick={() => open(n.id)}
				aria-label="Open {n.name}"
			>
				<span
					class="grid size-9 shrink-0 place-items-center overflow-hidden rounded-full bg-muted text-xs font-medium"
				>
					{#if n.photo}
						<MediaImage file={n.photo as MediaFile} alt="" class="size-9 object-cover" />
					{:else}{initials(n.name)}{/if}
				</span>
				<span class="min-w-0">
					<span class="block truncate text-sm font-medium">{n.name}</span>
					<span class="block truncate text-xs text-muted-foreground">{n.title}</span>
				</span>
			</button>
			{#if n.children.length > 0}
				<button
					type="button"
					class="shrink-0 rounded-full bg-muted px-1.5 text-[11px] text-muted-foreground hover:text-foreground"
					aria-expanded={!folded[n.id]}
					aria-label="{folded[n.id] ? 'Show' : 'Hide'} the {n.children
						.length} reporting to {n.name}"
					onclick={() => (folded[n.id] = !folded[n.id])}>{n.children.length}</button
				>
			{/if}
		</div>
		{#if n.children.length > 0 && !folded[n.id]}
			<div class="h-4 w-px bg-border"></div>
			<ul class="flex items-start gap-4 border-t border-border pt-0">
				{#each n.children as child (child.id)}
					<li class="relative flex flex-col items-center pt-4">
						<div class="absolute top-0 left-1/2 h-4 w-px bg-border"></div>
						{@render card(child)}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/snippet}

<div class="overflow-auto p-4" data-part="orgchart">
	{#if loading}
		<p class="text-sm text-muted-foreground">Drawing the chart…</p>
	{:else if failure}
		<p class="text-sm text-destructive" role="alert">{failure}</p>
	{:else if roots.length === 0}
		<p class="text-sm text-muted-foreground">No one to show.</p>
	{:else}
		<div class="flex min-w-max items-start justify-center gap-8">
			{#each roots as root (root.id)}{@render card(root)}{/each}
		</div>
	{/if}
</div>

<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import { boardGridTemplate, parseBoardLayout } from '$lib/pages/kanban';
	import { DashboardState, setDashboardState } from '$lib/pages/dashboardContext.svelte';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const dash = new DashboardState();
	setDashboardState(dash);

	const source = $derived(text(node.source));
	const colsAttr = $derived(Number(node.cols) || 0);
	const layoutWeights = $derived(parseBoardLayout(node.layout ?? node.style));
	const gridTemplate = $derived(boardGridTemplate(layoutWeights));

	const columns = $derived((node.children ?? []).filter((c) => c.type === 'column'));
	const loose = $derived((node.children ?? []).filter((c) => c.type !== 'column'));

	let latest = 0;

	async function load() {
		const current = ++latest;
		if (!source || !page) {
			dash.data = null;
			dash.loading = false;
			dash.error = null;
			return;
		}
		dash.loading = true;
		const result = await page.call(source, { ...page.params });
		if (current !== latest) return;
		dash.loading = false;
		if (
			result.ok &&
			result.data &&
			typeof result.data === 'object' &&
			!Array.isArray(result.data)
		) {
			dash.data = result.data as Record<string, unknown>;
			dash.error = null;
		} else if (result.ok) {
			dash.data = { value: result.data };
			dash.error = null;
		} else {
			dash.error = result.error;
			dash.data = dash.data ?? {};
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		void load();
	});

	const gridClass = $derived.by(() => {
		if (columns.length > 0) return 'gap-4';
		const n = colsAttr || 4;
		if (n <= 2) return 'grid gap-4 sm:grid-cols-2';
		if (n === 3) return 'grid gap-4 sm:grid-cols-2 lg:grid-cols-3';
		return 'grid gap-4 sm:grid-cols-2 lg:grid-cols-4';
	});
</script>

<div class="space-y-3">
	{#if dash.error && source}
		<p class="text-sm text-destructive" role="alert">{dash.error}</p>
	{/if}

	{#if columns.length > 0}
		<div
			class="grid gap-4"
			style={gridTemplate
				? `grid-template-columns: ${gridTemplate}`
				: `grid-template-columns: repeat(${columns.length}, minmax(0, 1fr))`}
		>
			{#each columns as col, i (col.title ?? col.name ?? i)}
				<section class="min-w-0 space-y-3 rounded-md border border-border bg-muted/20 p-3">
					{#if col.title || col.label}
						<h3 class="text-sm font-semibold text-foreground">
							{String(col.title ?? col.label)}
						</h3>
					{/if}
					{#each col.children ?? [] as child, j (child.name ?? `${child.type}-${j}`)}
						<PageRenderer node={child} />
					{/each}
				</section>
			{/each}
		</div>
		{#if loose.length > 0}
			<div class={gridClass}>
				{#each loose as child, i (child.name ?? `${child.type}-${i}`)}
					<PageRenderer node={child} />
				{/each}
			</div>
		{/if}
	{:else}
		<div class={gridClass}>
			{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
				<PageRenderer node={child} />
			{/each}
		</div>
	{/if}
</div>

<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { getDashboardState } from '$lib/pages/dashboardContext.svelte';
	import { getPageState, rowsOf, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const dash = getDashboardState();

	const title = $derived(String(node.title ?? node.name ?? 'Chart'));
	// XML `type="bar"` on <chart> becomes `viewType` (same remap as <view type="…">).
	const kind = $derived.by(() => {
		const explicit =
			text(node.viewType) ?? text(node.view_type) ?? text(node.chartType) ?? text(node.kind);
		return (explicit ?? 'bar').toLowerCase();
	});
	const source = $derived(text(node.source));
	const categoryField = $derived(text(node.category) ?? 'category');
	const valueField = $derived(text(node.value) ?? 'value');
	const dataField = $derived(text(node.field));

	let ownRows = $state<Record<string, unknown>[] | null>(null);
	let ownError = $state<string | null>(null);
	let ownLoading = $state(false);
	let latest = 0;

	function rowsFromPayload(data: unknown): Record<string, unknown>[] {
		if (dataField && data && typeof data === 'object' && !Array.isArray(data)) {
			const nested = (data as Record<string, unknown>)[dataField];
			const fromField = rowsOf(nested);
			if (fromField.length > 0 || Array.isArray(nested)) return fromField;
		}
		return rowsOf(data);
	}

	async function loadOwn() {
		const current = ++latest;
		if (!source || !page) {
			ownRows = null;
			return;
		}
		ownLoading = true;
		const result = await page.call(source, { ...page.params });
		if (current !== latest) return;
		ownLoading = false;
		if (result.ok) {
			ownRows = rowsFromPayload(result.data);
			ownError = null;
		} else {
			ownError = result.error;
			ownRows = [];
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		if (source) void loadOwn();
	});

	const points = $derived.by(() => {
		let rows: Record<string, unknown>[] = [];
		if (source && ownRows) {
			rows = ownRows;
		} else if (dataField && dash?.data) {
			const raw = dash.data[dataField];
			rows = rowsOf(raw);
		} else if (dash?.data) {
			const raw = dash.data.series ?? dash.data.rows ?? dash.data.data;
			rows = rowsOf(raw);
		}
		return rows.map((row) => ({
			label: String(row[categoryField!] ?? row.label ?? row.name ?? ''),
			value: Number(row[valueField!] ?? row.value ?? 0) || 0
		}));
	});

	const max = $derived(Math.max(1, ...points.map((p) => p.value)));
	const total = $derived(points.reduce((a, p) => a + p.value, 0) || 1);
	const loading = $derived(source ? ownLoading : Boolean(dash?.loading && points.length === 0));
	const error = $derived(ownError ?? (!source ? dash?.error : null) ?? null);

	// The theme's chart palette (`--chart-1` … `--chart-5`).
	const pieColors = ['bg-chart-1', 'bg-chart-2', 'bg-chart-3', 'bg-chart-4', 'bg-chart-5'];
	const chartVars = pieColors.map((_, i) => `var(--chart-${i + 1})`);
</script>

<div class="rounded-md border border-border bg-card p-4" data-part="dashboard.chart">
	<p class="mb-3 text-sm font-medium text-foreground">{title}</p>

	{#if loading}
		<Skeleton class="h-40 w-full" />
	{:else if error}
		<p class="text-sm text-destructive" role="alert">{error}</p>
	{:else if points.length === 0}
		<div
			class="flex h-40 items-center justify-center rounded-md bg-muted/40 text-sm text-muted-foreground"
		>
			No chart data
		</div>
	{:else if kind === 'pie'}
		<div class="flex flex-col gap-3 sm:flex-row sm:items-center">
			<div
				class="mx-auto size-36 rounded-full"
				style="background: conic-gradient({points
					.map((p, i) => {
						const start = points.slice(0, i).reduce((a, x) => a + x.value, 0) / total;
						const end = start + p.value / total;
						return `${chartVars[i % chartVars.length]} ${start * 360}deg ${end * 360}deg`;
					})
					.join(', ')})"
				role="img"
				aria-label={title}
			></div>
			<ul class="space-y-1 text-xs">
				{#each points as p, i (p.label)}
					<li class="flex items-center gap-2">
						<span class="size-2.5 rounded-sm {pieColors[i % pieColors.length]}"></span>
						<span class="text-muted-foreground">{p.label}</span>
						<span class="ml-auto font-medium tabular-nums">{p.value}</span>
					</li>
				{/each}
			</ul>
		</div>
	{:else if kind === 'line'}
		<svg
			viewBox="0 0 100 40"
			class="h-40 w-full text-primary"
			preserveAspectRatio="none"
			role="img"
			aria-label={title}
		>
			<polyline
				fill="none"
				stroke="currentColor"
				stroke-width="1.5"
				points={points
					.map((p, i) => {
						const x = points.length === 1 ? 50 : (i / (points.length - 1)) * 100;
						const y = 38 - (p.value / max) * 34;
						return `${x},${y}`;
					})
					.join(' ')}
			/>
			{#each points as p, i (p.label)}
				{@const x = points.length === 1 ? 50 : (i / (points.length - 1)) * 100}
				{@const y = 38 - (p.value / max) * 34}
				<circle cx={x} cy={y} r="1.2" fill="currentColor" />
			{/each}
		</svg>
		<div class="mt-1 flex justify-between gap-1 text-xs text-muted-foreground">
			{#each points as p (p.label)}
				<span class="truncate">{p.label}</span>
			{/each}
		</div>
	{:else}
		<!-- bar (default) -->
		<div class="flex h-40 items-end gap-2" role="img" aria-label={title}>
			{#each points as p (p.label)}
				<div class="flex min-w-0 flex-1 flex-col items-center gap-1">
					<span class="text-xs text-muted-foreground tabular-nums">{p.value}</span>
					<div
						class="w-full rounded-t-sm bg-chart-1"
						style="height: {Math.max(4, (p.value / max) * 100)}%"
						title="{p.label}: {p.value}"
					></div>
					<span class="w-full truncate text-center text-xs text-muted-foreground">{p.label}</span>
				</div>
			{/each}
		</div>
	{/if}
</div>

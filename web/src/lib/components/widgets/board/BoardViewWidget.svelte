<script lang="ts">
	import { untrack } from 'svelte';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';
	import type { PageNode } from '$lib/dsl/types';
	import BoardCard from './BoardCard.svelte';
	import BoardFilterBar from './BoardFilterBar.svelte';
	import DrillDialog from './DrillDialog.svelte';
	import type { BoardData, BoardItem, DrillData } from './types';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const drillFn = $derived(text(node.drill));

	let data = $state<BoardData | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	/** What the filter bar holds now, by filter name; empty until the first answer names them. */
	let values = $state<Record<string, unknown>>({});
	let primed = false;
	let latest = 0;
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function load() {
		if (!source || !page) {
			loading = false;
			return;
		}
		const current = ++latest;
		const payload: Record<string, unknown> = { ...page.params };
		// Until the bar has been drawn the server applies each filter's default.
		if (primed) payload.filters = $state.snapshot(values);
		const result = await page.call(source, payload);
		if (current !== latest) return;
		loading = false;
		if (result.ok && result.data && typeof result.data === 'object') {
			data = result.data as BoardData;
			error = null;
			if (!primed) {
				values = Object.fromEntries(data.filters.map((f) => [f.name, f.value]));
				primed = true;
			}
		} else if (!result.ok) {
			error = result.error ?? 'The board could not be loaded.';
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		untrack(() => void load());
	});

	// A dashboard may ask to be refreshed on a timer.
	$effect(() => {
		const secs = data?.refresh_secs ?? 0;
		if (secs < 5) return;
		const handle = setInterval(() => untrack(() => void load()), secs * 1000);
		return () => clearInterval(handle);
	});

	function change(name: string, value: unknown, immediate: boolean) {
		values = { ...values, [name]: value };
		clearTimeout(timer);
		if (immediate) void load();
		else timer = setTimeout(() => void load(), 350);
	}

	// ---- drill-down ----
	let drillOpen = $state(false);
	let drillItem = $state<BoardItem | null>(null);
	let drillSegment = $state<Record<string, unknown>>({});
	let drillLabel = $state('');
	let drillData = $state<DrillData | null>(null);
	let drillLoading = $state(false);
	let drillError = $state<string | null>(null);
	let drillLatest = 0;

	async function loadDrill(offset: number) {
		if (!drillFn || !page || !drillItem) return;
		const current = ++drillLatest;
		drillLoading = true;
		const result = await page.call(drillFn, {
			...page.params,
			chart: drillItem.chart ?? drillItem.id,
			segment: $state.snapshot(drillSegment),
			filters: $state.snapshot(values),
			limit: 25,
			offset
		});
		if (current !== drillLatest) return;
		drillLoading = false;
		if (result.ok) {
			drillData = result.data as DrillData;
			drillError = null;
		} else {
			drillError = result.error ?? 'The records could not be loaded.';
		}
	}

	function openDrill(item: BoardItem, segment: Record<string, unknown>, summary?: string) {
		drillLabel = summary ?? '';
		if (!drillFn) return;
		drillItem = item;
		drillSegment = segment;
		drillData = null;
		drillError = null;
		drillOpen = true;
		void loadDrill(0);
	}

	const drillTitle = $derived(drillItem?.card?.title || drillItem?.title || 'Records');
	const drillSummary = $derived(
		drillLabel ||
			Object.entries(drillSegment)
				.map(
					([field, value]) =>
						`${field.replaceAll('_', ' ')}: ${value === null ? '(none)' : String(value)}`
				)
				.join(' · ') ||
			'All records'
	);
</script>

<div class="space-y-4" data-part="board">
	{#if data?.description}
		<p class="text-sm text-muted-foreground">{data.description}</p>
	{/if}

	{#if data}
		<BoardFilterBar filters={data.filters} {values} onchange={change} />
	{/if}

	{#if error}
		<p class="text-sm text-destructive" role="alert">{error}</p>
	{/if}

	{#if loading && !data}
		<div class="grid gap-4 md:grid-cols-2">
			<Skeleton class="h-56 w-full" />
			<Skeleton class="h-56 w-full" />
		</div>
	{:else if data}
		<div class="grid grid-cols-1 gap-4 md:grid-cols-12">
			{#each data.items as item (item.id)}
				<div class="min-w-0 md:[grid-column:span_var(--span)]" style="--span: {item.width ?? 6}">
					<BoardCard {item} ondrill={drillFn ? openDrill : undefined} />
				</div>
			{:else}
				<p class="text-sm text-muted-foreground md:col-span-12">Nothing to show yet.</p>
			{/each}
		</div>
	{/if}
</div>

<DrillDialog
	bind:open={drillOpen}
	title={drillTitle}
	summary={drillSummary}
	loading={drillLoading}
	error={drillError}
	data={drillData}
	onpage={(offset) => void loadDrill(offset)}
/>

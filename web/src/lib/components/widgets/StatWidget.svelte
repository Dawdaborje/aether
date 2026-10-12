<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import * as Card from '$lib/components/ui/card/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { getDashboardState } from '$lib/pages/dashboardContext.svelte';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const dash = getDashboardState();

	const label = $derived(String(node.label ?? node.title ?? 'Metric'));
	const field = $derived(text(node.field) ?? text(node.name));
	const source = $derived(text(node.source));
	const staticValue = $derived(node.value);

	let ownData = $state<unknown>(null);
	let ownLoading = $state(false);
	let ownError = $state<string | null>(null);
	let latest = 0;

	async function loadOwn() {
		const current = ++latest;
		if (!source || !page) {
			ownData = null;
			return;
		}
		ownLoading = true;
		const result = await page.call(source, { ...page.params });
		if (current !== latest) return;
		ownLoading = false;
		if (result.ok) {
			ownData = result.data;
			ownError = null;
		} else {
			ownError = result.error;
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		if (source) void loadOwn();
	});

	function readValue(data: unknown): unknown {
		if (staticValue !== undefined && staticValue !== null && !source && !field) return staticValue;
		if (field && data && typeof data === 'object' && !Array.isArray(data)) {
			return (data as Record<string, unknown>)[field];
		}
		if (field && dash?.data) return dash.data[field];
		if (staticValue !== undefined && staticValue !== null) return staticValue;
		if (typeof data === 'number' || typeof data === 'string') return data;
		return undefined;
	}

	const display = $derived.by(() => {
		const fromOwn = source ? readValue(ownData) : undefined;
		if (fromOwn !== undefined) return fromOwn;
		if (field && dash?.data) return dash.data[field];
		if (staticValue !== undefined && staticValue !== null) return staticValue;
		return undefined;
	});

	const loading = $derived(source ? ownLoading : Boolean(dash?.loading && field && display === undefined));
	const error = $derived(ownError ?? (field && !source ? dash?.error : null) ?? null);

	function format(value: unknown): string {
		if (value === null || value === undefined) return '—';
		if (typeof value === 'number') {
			return value.toLocaleString(undefined, { maximumFractionDigits: 2 });
		}
		return String(value);
	}
</script>

<Card.Root size="sm" class="h-full">
	<Card.Header class="gap-1 pb-0">
		<Card.Description class="text-xs font-medium tracking-wide uppercase">{label}</Card.Description>
		{#if loading}
			<Skeleton class="mt-1 h-8 w-20" />
		{:else if error}
			<p class="text-sm text-destructive" role="alert">{error}</p>
		{:else}
			<Card.Title class="text-2xl font-semibold tabular-nums">{format(display)}</Card.Title>
		{/if}
	</Card.Header>
</Card.Root>

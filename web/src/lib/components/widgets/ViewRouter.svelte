<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import ListViewWidget from './ListViewWidget.svelte';
	import FormViewWidget from './FormViewWidget.svelte';
	import KanbanViewWidget from './KanbanViewWidget.svelte';
	import DashboardViewWidget from './DashboardViewWidget.svelte';
	import FallbackWidget from './FallbackWidget.svelte';

	let { node }: { node: PageNode } = $props();

	const viewType = $derived(
		typeof node.viewType === 'string'
			? node.viewType
			: typeof node.view_type === 'string'
				? node.view_type
				: 'list'
	);
</script>

{#if viewType === 'list' || viewType === 'tree' || viewType === 'pivot'}
	<ListViewWidget {node} />
{:else if viewType === 'form'}
	<FormViewWidget {node} />
{:else if viewType === 'kanban'}
	<KanbanViewWidget {node} />
{:else if viewType === 'dashboard'}
	<DashboardViewWidget {node} />
{:else}
	<FallbackWidget {node} />
{/if}

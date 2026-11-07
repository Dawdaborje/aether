<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { browser } from '$app/environment';
	import ErrorCard from '$lib/components/errors/ErrorCard.svelte';

	let { node }: { node: PageNode } = $props();

	const show = $derived(browser && import.meta.env.DEV);
</script>

{#if show}
	<ErrorCard
		title="Unknown widget"
		message={`No widget is registered for "${node.type}"${node.name ? ` (${String(node.name)})` : ''}.`}
	/>
{/if}

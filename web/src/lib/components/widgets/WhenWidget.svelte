<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';
	import PageRenderer from './PageRenderer.svelte';

	/**
	 * `<when source="channel_header" field="kind" is="channel">…</when>`: shows its children only when what `source`
	 * answers has `field` equal to `is` (or, with `isnot`, different from it). `source` is a function called with the
	 * page's values that answers an object. Nothing is shown while the answer is awaited or when the call fails.
	 * It lets one page draw a conversation one way for channels and another way for chats.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	let value = $state<string | null>(null);
	let latest = 0;

	$effect(() => {
		void page?.plugin;
		void JSON.stringify(page?.args ?? {});
		const fn = text(node.source);
		const field = text(node.field);
		if (!fn || !field || !page) return;
		const current = ++latest;
		value = null;
		void page.call(fn, page.args).then((result) => {
			if (current !== latest) return;
			const data = result.ok && result.data && typeof result.data === 'object' ? result.data : null;
			const found = data ? (data as Record<string, unknown>)[field] : undefined;
			value = found === undefined || found === null ? null : String(found);
		});
	});

	const show = $derived.by(() => {
		if (value === null) return false;
		const is = text(node.is);
		const isnot = text(node.isnot);
		if (is !== undefined) return value === is;
		if (isnot !== undefined) return value !== isnot;
		return true;
	});
</script>

{#if show}
	{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
		<PageRenderer node={child} />
	{/each}
{/if}

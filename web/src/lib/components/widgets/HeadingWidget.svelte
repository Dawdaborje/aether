<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { useLiveTopic } from '$lib/pages/liveTopic.svelte';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';
	import PageRenderer from './PageRenderer.svelte';

	/**
	 * `<heading source="channel_header" title_field="title" subtitle_field="subtitle">`: the title of what the page
	 * shows, with its buttons (the children) on the right. `source` is a function called with the page's values that
	 * answers an object; without it `title` and `subtitle` are used as written. With `topic` it asks again when
	 * something is published there. With `logo` (an image address) the logo is shown before the title.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const live = useLiveTopic(() => node.topic);
	let data = $state<Record<string, unknown>>({});
	let latest = 0;

	$effect(() => {
		void page?.plugin;
		void JSON.stringify(page?.args ?? {});
		void page?.tick;
		void live.n;
		const fn = source;
		if (!fn || !page) return;
		const current = ++latest;
		void page.call(fn, page.args).then((result) => {
			if (current !== latest) return;
			data =
				result.ok && result.data && typeof result.data === 'object'
					? (result.data as Record<string, unknown>)
					: {};
		});
	});

	const title = $derived(String(data[text(node.title_field) ?? 'title'] ?? text(node.title) ?? ''));
	const subtitle = $derived(
		String(data[text(node.subtitle_field) ?? 'subtitle'] ?? text(node.subtitle) ?? '')
	);
</script>

<header
	class="flex shrink-0 items-center gap-3 border-b border-border px-4 py-3"
	data-part="heading"
>
	{#if text(node.logo)}
		<img
			src={text(node.logo)}
			alt=""
			class="size-8 shrink-0 rounded-md object-contain"
			data-part="heading-logo"
		/>
	{/if}
	<div class="min-w-0 flex-1">
		<h2 class="truncate type-heading" data-part="heading-title">{title}</h2>
		{#if subtitle}<p class="truncate text-sm text-muted-foreground">{subtitle}</p>{/if}
	</div>
	<div class="flex shrink-0 items-center gap-1">
		{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
			<PageRenderer node={child} />
		{/each}
	</div>
</header>

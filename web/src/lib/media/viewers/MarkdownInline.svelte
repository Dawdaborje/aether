<script lang="ts">
	import { navHref } from '$lib/components/layout/nav';
	import type { Inline } from './markdown';
	import MarkdownInline from './MarkdownInline.svelte';

	let { items }: { items: Inline[] } = $props();
</script>

{#each items as item, i (i)}{#if item.t === 'text'}{item.v}{:else if item.t === 'code'}<code
			class="rounded bg-muted px-1 text-[0.85em]">{item.v}</code
		>{:else if item.t === 'strong'}<strong><MarkdownInline items={item.c} /></strong
		>{:else if item.t === 'em'}<em><MarkdownInline items={item.c} /></em
		>{:else if item.t === 'del'}<del><MarkdownInline items={item.c} /></del
		>{:else if item.t === 'link' && item.app}<a
			href={navHref(item.href)}
			class="text-primary underline"><MarkdownInline items={item.c} /></a
		>{:else if item.t === 'link'}<a
			href={item.href}
			target="_blank"
			rel="noopener noreferrer nofollow"
			class="text-primary underline"><MarkdownInline items={item.c} /></a
		>{/if}{/each}

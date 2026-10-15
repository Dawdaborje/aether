<script lang="ts">
	import { page } from '$app/state';
	import { appIcon } from '$lib/components/apps/icons';
	import type { LayoutBlock } from '$lib/theme/types';
	import { isActive, navHref } from '../../nav';
	import { textProp } from '../visibility';

	let { block, vertical = false }: { block: LayoutBlock; vertical?: boolean } = $props();

	const href = $derived(textProp(block, 'href', '/'));
	const label = $derived(textProp(block, 'label'));
	const Icon = $derived(appIcon(textProp(block, 'icon')));
	const external = $derived(!href.startsWith('/'));
	const current = $derived(isActive({ label, href }, page.url.pathname));
</script>

<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
<a
	href={navHref(href)}
	target={external ? '_blank' : undefined}
	rel={external ? 'noopener noreferrer' : undefined}
	aria-current={current ? 'page' : undefined}
	class="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring aria-[current=page]:bg-region-active aria-[current=page]:font-medium aria-[current=page]:text-region-foreground {vertical
		? 'w-full'
		: ''}"
>
	{#if Icon}<Icon class="size-4 shrink-0" />{/if}
	<span class="truncate">{label}</span>
</a>
<!-- eslint-enable svelte/no-navigation-without-resolve -->

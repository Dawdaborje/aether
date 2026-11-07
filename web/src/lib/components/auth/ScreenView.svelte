<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { brandingStore } from '$lib/branding/brandingStore.svelte';
	import { themeStore } from '$lib/theme/themeStore.svelte';
	import type { ScreenBlock, ScreenDef } from '$lib/theme/types';

	/**
	 * A sign-in screen a theme ships as XML (`screens/<name>.xml`): panels of blocks. The forms
	 * are the app's own and come in as snippets, so they behave as they do on the built-in screen.
	 */
	let {
		def,
		forms = {}
	}: { def: ScreenDef; forms?: Partial<Record<ScreenBlock['type'], Snippet>> } = $props();

	const logo = $derived(themeStore.theme.shell?.branding?.logo ?? null);
	const TONES: Record<string, string> = {
		default: 'bg-background text-foreground',
		muted: 'bg-muted text-foreground',
		primary: 'bg-primary text-primary-foreground'
	};
	const aside = $derived(def.aside ?? null);
	const asideRight = $derived(aside?.side === 'right');
	const prop = (block: ScreenBlock, key: string) => block.props?.[key];
	const HEADINGS: Record<number, string> = {
		1: 'type-display text-3xl',
		2: 'type-headline text-xl',
		3: 'type-title'
	};
</script>

{#snippet blocks(list: ScreenBlock[])}
	{#each list as block, i (i)}
		{#if block.type === 'brand'}
			<p class="flex items-center gap-2 text-sm font-semibold tracking-wide" data-part="auth.brand">
				{#if logo && prop(block, 'show_logo') !== false}
					<img src={logo} alt="" class="size-7" />
				{/if}
				{prop(block, 'label') ?? brandingStore.appName}
			</p>
		{:else if block.type === 'heading'}
			{@const level = Number(prop(block, 'level') ?? 1)}
			{#if level === 1}
				<h1 class={HEADINGS[1]}>{prop(block, 'text')}</h1>
			{:else if level === 2}
				<h2 class={HEADINGS[2]}>{prop(block, 'text')}</h2>
			{:else}
				<h3 class={HEADINGS[3]}>{prop(block, 'text')}</h3>
			{/if}
		{:else if block.type === 'text'}
			<p class="text-sm {prop(block, 'muted') ? 'opacity-70' : ''}">{prop(block, 'text')}</p>
		{:else if block.type === 'image'}
			<img
				src={String(prop(block, 'src') ?? '')}
				alt={String(prop(block, 'alt') ?? '')}
				class="max-h-64 w-full object-contain"
			/>
		{:else if block.type === 'link'}
			<!-- eslint-disable svelte/no-navigation-without-resolve -- the href is app-relative or absolute, checked by the server -->
			<a
				class="text-sm underline"
				href={String(prop(block, 'href')).startsWith('/')
					? resolve(String(prop(block, 'href')) as '/')
					: String(prop(block, 'href'))}>{prop(block, 'label')}</a
			>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		{:else if block.type === 'divider'}
			<hr class="border-border" />
		{:else if block.type === 'spacer'}
			<div class="flex-1"></div>
		{:else if forms[block.type]}
			{@render forms[block.type]!()}
		{/if}
	{/each}
{/snippet}

<div
	data-part="auth.screen"
	data-screen={def.name}
	data-layout={def.layout}
	class="min-h-screen w-full bg-background {def.layout === 'split' && aside
		? 'grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]'
		: 'flex flex-col'}"
>
	{#if aside}
		<aside
			data-part="auth.aside"
			class="flex flex-col justify-between gap-6 p-8 lg:p-12 {TONES[aside.tone ?? 'default']} {def.layout ===
			'split'
				? `hidden lg:flex ${asideRight ? 'order-2' : ''}`
				: ''}"
		>
			{@render blocks(aside.blocks)}
		</aside>
	{/if}
	<main
		data-part="auth.main"
		class="flex flex-1 items-center justify-center px-6 py-12"
	>
		<div class="flex w-full max-w-sm flex-col gap-5">
			{@render blocks(def.main.blocks)}
		</div>
	</main>
</div>

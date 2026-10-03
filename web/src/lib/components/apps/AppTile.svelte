<script lang="ts">
	import { navHref } from '$lib/components/layout/nav';
	import { appIcon } from './icons';
	import type { AppTileData } from './types';

	let { app }: { app: AppTileData } = $props();

	const Icon = $derived(appIcon(app.icon));
</script>

<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the app's base path -->
<a
	href={navHref(app.route)}
	class="group flex w-full flex-col items-center gap-2.5 rounded-xl p-3 text-center outline-none transition hover:bg-card hover:shadow-sm focus-visible:ring-2 focus-visible:ring-ring"
>
	<span
		class="flex size-14 items-center justify-center rounded-xl border border-border bg-card text-foreground/75 transition group-hover:border-primary/50 group-hover:text-primary"
		aria-hidden="true"
	>
		{#if Icon}
			<Icon class="size-6" />
		{:else}
			<span class="text-xl font-semibold">{app.label.charAt(0).toUpperCase()}</span>
		{/if}
	</span>
	<span class="max-w-full space-y-0.5">
		<span class="block truncate text-[13px] font-medium text-foreground">{app.label}</span>
		{#if app.description}
			<span class="line-clamp-2 block text-[11px] leading-snug text-muted-foreground">{app.description}</span>
		{/if}
	</span>
</a>
<!-- eslint-enable svelte/no-navigation-without-resolve -->

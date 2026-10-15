<script lang="ts">
	import { appIcon } from './icons';
	import type { AppCategory, AppTileData } from './types';

	let {
		category,
		count,
		apps = [],
		onclick
	}: {
		category: AppCategory;
		count: number;
		/** Member apps — used for a mosaic when the folder has no logo/icon. */
		apps?: AppTileData[];
		onclick: () => void;
	} = $props();

	const Icon = $derived(appIcon(category.icon));
	let logoFailed = $state(false);
	const showLogo = $derived(Boolean(category.logo_url) && !logoFailed);

	/** Up to four members with a logo or Lucide icon for the mosaic fallback. */
	const previews = $derived(
		apps.filter((app) => Boolean(app.logo_url) || Boolean(app.icon)).slice(0, 4)
	);

	$effect(() => {
		void category.logo_url;
		logoFailed = false;
	});
</script>

<button
	type="button"
	class="group flex w-full flex-col items-center gap-3 rounded-xl p-4 text-center transition-colors outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
	{onclick}
>
	<span
		class="flex size-14 items-center justify-center overflow-hidden rounded-2xl bg-card ring-1 ring-border sm:size-16"
		aria-hidden="true"
	>
		{#if showLogo && category.logo_url}
			<img
				src={category.logo_url}
				alt=""
				class="size-10 object-contain sm:size-12"
				onerror={() => (logoFailed = true)}
			/>
		{:else if Icon}
			<Icon class="size-7 text-primary" />
		{:else if previews.length > 0}
			<span class="grid size-10 grid-cols-2 gap-0.5 sm:size-12 sm:gap-1">
				{#each previews as app (app.plugin)}
					{@const MemberIcon = appIcon(app.icon)}
					<span
						class="flex items-center justify-center overflow-hidden rounded bg-primary/10 text-primary"
					>
						{#if app.logo_url}
							<img src={app.logo_url} alt="" class="size-full object-contain p-0.5" />
						{:else if MemberIcon}
							<MemberIcon class="size-3" />
						{/if}
					</span>
				{/each}
			</span>
		{:else}
			<span class="text-xl font-semibold text-primary">
				{category.label.charAt(0).toUpperCase()}
			</span>
		{/if}
	</span>
	<div class="max-w-full space-y-0.5">
		<span class="block truncate text-sm font-medium">{category.label}</span>
		<span class="block text-xs text-muted-foreground">
			{count} app{count === 1 ? '' : 's'}
		</span>
	</div>
</button>

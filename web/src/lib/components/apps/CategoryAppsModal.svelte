<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import AppTile from './AppTile.svelte';
	import type { AppCategory, AppTileData } from './types';

	let {
		open = $bindable(false),
		category,
		apps
	}: {
		open?: boolean;
		category: AppCategory | null;
		apps: AppTileData[];
	} = $props();

	const Icon = $derived(category ? iconRef(category.icon) : undefined);
	let logoFailed = $state(false);
	const headerPreview = $derived(
		apps.filter((app) => Boolean(app.logo_url) || Boolean(app.icon)).slice(0, 4)
	);

	$effect(() => {
		void category?.logo_url;
		logoFailed = false;
	});
</script>

<Dialog.Root bind:open>
	{#if category}
		<!-- Frosted, see-through panel sized for about five tiles per row. -->
		<Dialog.Content
			overlayClass="bg-black/10"
			class="flex max-h-[min(85vh,40rem)] w-full flex-col border border-border/60 bg-popover/70 pb-4 shadow-2xl backdrop-blur-2xl backdrop-saturate-150 sm:max-w-3xl"
		>
			<Dialog.Header class="shrink-0">
				<div class="flex items-start gap-3 pr-8">
					<span
						class="flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-xl bg-primary/10 text-primary ring-1 ring-primary/15"
						aria-hidden="true"
					>
						{#if category.logo_url && !logoFailed}
							<img
								src={category.logo_url}
								alt=""
								class="size-10 object-contain"
								onerror={() => (logoFailed = true)}
							/>
						{:else if Icon}
							<AppIcon name={Icon} class="size-6" />
						{:else if headerPreview.length > 0}
							<span class="grid size-10 grid-cols-2 gap-0.5 p-0.5">
								{#each headerPreview as app (app.plugin)}
									{@const MemberIcon = iconRef(app.icon)}
									<span
										class="flex items-center justify-center overflow-hidden rounded-sm bg-primary/15"
									>
										{#if app.logo_url}
											<img src={app.logo_url} alt="" class="size-full object-contain" />
										{:else if MemberIcon}
											<AppIcon name={MemberIcon} class="size-2.5" />
										{/if}
									</span>
								{/each}
							</span>
						{:else}
							<span class="text-xl font-semibold">{category.label.charAt(0).toUpperCase()}</span>
						{/if}
					</span>
					<div class="min-w-0 space-y-1">
						<Dialog.Title class="text-left">{category.label}</Dialog.Title>
						<Dialog.Description class="text-left">
							{apps.length} app{apps.length === 1 ? '' : 's'} in this folder
						</Dialog.Description>
					</div>
				</div>
			</Dialog.Header>

			{#if apps.length === 0}
				<p class="text-sm text-muted-foreground">No apps in this group.</p>
			{:else}
				<!-- Only the list scrolls, so the header and close button stay put. -->
				<ul
					class="-mx-4 grid min-h-0 flex-1 grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2 overflow-y-auto px-4"
					onclick={(e) => {
						if ((e.target as HTMLElement).closest('a')) open = false;
					}}
				>
					{#each apps as app (app.plugin)}
						<li><AppTile {app} /></li>
					{/each}
				</ul>
			{/if}
		</Dialog.Content>
	{/if}
</Dialog.Root>

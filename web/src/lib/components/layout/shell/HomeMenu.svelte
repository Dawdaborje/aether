<script lang="ts">
	import { page } from '$app/state';
	import BuildingIcon from '@lucide/svelte/icons/building-2';
	import { appIcon } from '$lib/components/apps/icons';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { isActive, manageNavItems, navHref } from '../nav';

	let open = $state(false);

	/** Settings, Users, … — only what this account is allowed to open. */
	const manageItems = $derived(
		manageNavItems(
			authSession.isDeveloper,
			authSession.canManageUsers,
			authSession.canManageOrg,
			authSession.isOrgDeveloper
		)
	);

	// Navigating anywhere closes the menu.
	$effect(() => {
		void page.url.pathname;
		open = false;
	});
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<button
	type="button"
	onclick={() => (open = !open)}
	aria-label="Home menu"
	aria-expanded={open}
	title="Home menu"
	class="group flex size-9 items-center justify-center rounded-md transition outline-none hover:bg-region-hover focus-visible:ring-2 focus-visible:ring-region-ring aria-expanded:bg-region-hover"
>
	<span class="grid grid-cols-3 gap-[3px]" aria-hidden="true">
		{#each Array(9) as _, i (i)}
			<span
				class="size-[5px] rounded-[1.5px] bg-region-muted-foreground transition group-hover:bg-region-hover-foreground"
			></span>
		{/each}
	</span>
</button>

{#if open}
	<!-- Click-away backdrop below the bar. -->
	<button
		type="button"
		tabindex="-1"
		aria-label="Close home menu"
		onclick={() => (open = false)}
		class="fixed inset-x-0 top-12 bottom-0 z-40 cursor-default bg-foreground/10"
	></button>

	<div
		role="presentation"
		onclick={(e) => (e.target as HTMLElement).closest('a') && (open = false)}
		class="fixed top-12 bottom-0 left-0 z-50 w-full max-w-4xl overflow-y-auto border-r border-border bg-popover text-popover-foreground shadow-md"
	>
		<div class="flex w-full flex-col gap-6 px-6 py-6 md:flex-row md:gap-8">
			<div class="w-full shrink-0 space-y-5 md:w-64">
				{#if orgStore.currentName}
					<section>
						<h2 class="px-3 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Organization
						</h2>
						<button
							type="button"
							disabled={!orgStore.canSwitch}
							onclick={() => {
								open = false;
								orgStore.openSwitcher();
							}}
							class="flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left text-sm transition enabled:hover:bg-accent disabled:cursor-default"
						>
							<BuildingIcon class="size-5 text-muted-foreground" />
							<span class="flex-1 truncate font-medium">{orgStore.currentName}</span>
							{#if orgStore.canSwitch}<span class="text-xs text-muted-foreground">Switch</span>{/if}
						</button>
					</section>
				{/if}

				{#if manageItems.length > 0}
					<section>
						<h2 class="px-3 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Navigate
						</h2>
						<ul class="space-y-0.5">
							{#each manageItems as item (item.href ?? item.label)}
								{@const Icon = appIcon(item.icon)}
								<li>
									<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
									<a
										href={item.href ? navHref(item.href) : '#'}
										aria-current={isActive(item, page.url.pathname) ? 'page' : undefined}
										class="flex items-center gap-3 rounded-md px-3 py-2.5 text-sm transition hover:bg-accent aria-[current=page]:bg-accent aria-[current=page]:font-semibold"
									>
										{#if Icon}<Icon class="size-5 text-muted-foreground" />{/if}
										{item.label}
									</a>
									<!-- eslint-enable svelte/no-navigation-without-resolve -->
								</li>
							{/each}
						</ul>
					</section>
				{/if}
			</div>

			<section class="min-w-0 flex-1">
				<div class="flex items-center justify-between gap-2 px-3 pb-2">
					<h2 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">Apps</h2>
					<!-- eslint-disable svelte/no-navigation-without-resolve -- navHref() applies the base path -->
					<a
						href={navHref('/apps')}
						class="rounded-md bg-primary/10 px-2 py-1 text-xs font-medium text-primary hover:bg-primary/15"
					>
						All apps
					</a>
					<!-- eslint-enable svelte/no-navigation-without-resolve -->
				</div>
				{#if appsStore.apps === null}
					<p class="px-3 text-sm text-muted-foreground">Loading apps…</p>
				{:else if appsStore.apps.length === 0}
					<p class="px-3 text-sm text-muted-foreground">No apps installed yet.</p>
				{:else}
					<ul class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-3">
						{#each appsStore.apps as app (app.plugin)}
							<li><AppTile {app} size="compact" /></li>
						{/each}
					</ul>
				{/if}
			</section>
		</div>
	</div>
{/if}

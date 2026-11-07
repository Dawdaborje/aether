<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import { authSession } from '$lib/auth/session.svelte';
	import { cn } from '$lib/utils.js';
	import type { CatalogGroup } from '$lib/settings/api';

	let { groups = [] }: { groups?: CatalogGroup[] } = $props();

	const iconFor = (slug: string) => {
		switch (slug) {
			case 'security':
				return 'shield';
			case 'notifications':
				return 'bell';
			case 'appearance':
				return 'palette';
			default:
				return 'settings';
		}
	};

	const links = $derived([
		...groups.map((g) => ({
			label: g.label.replace(/ Settings$/i, ''),
			href: `${base}/desk/settings/${g.slug}`,
			slug: g.slug,
			icon: iconFor(g.slug)
		})),
		...(authSession.isDeveloper
			? [
					{
						label: 'Appearance',
						href: `${base}/desk/settings/appearance`,
						slug: 'appearance',
						icon: 'palette'
					}
				]
			: [])
	]);

	const activePath = $derived($page.url.pathname);

	const item =
		'flex h-12 items-center gap-3 border-l-[3px] border-transparent px-4 text-sm text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground';
</script>

<nav class="sticky top-0 h-full min-w-56 border-r border-sidebar-border bg-sidebar">
	<a href={`${base}/desk/settings`} class={cn(item, 'font-semibold')}>
		<AppIcon name="settings" class="size-4" />
		<span>Settings</span>
	</a>
	<p class="px-4 pt-4 pb-2 text-xs tracking-[0.32px] text-muted-foreground">Configuration</p>
	<ul>
		{#each links as link (link.href)}
			{@const active = activePath === link.href || activePath.startsWith(link.href + '/')}
			<li>
				<a
					href={link.href}
					title={link.label}
					aria-current={active ? 'page' : undefined}
					class={cn(
						item,
						active &&
							'border-l-sidebar-primary bg-sidebar-accent font-semibold text-sidebar-accent-foreground'
					)}
				>
					<AppIcon name={link.icon} class="size-4" />
					<span>{link.label}</span>
				</a>
			</li>
		{/each}
	</ul>
</nav>

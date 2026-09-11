<script lang="ts">
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import {
		BellIcon,
		CogIcon,
		PaletteIcon,
		ShieldIcon
	} from '@lucide/svelte';
	import { Navigation } from '@skeletonlabs/skeleton-svelte';
	import type { CatalogGroup } from '$lib/settings/api';

	let { groups = [] }: { groups?: CatalogGroup[] } = $props();

	const iconFor = (slug: string) => {
		switch (slug) {
			case 'security':
				return ShieldIcon;
			case 'notifications':
				return BellIcon;
			case 'appearance':
				return PaletteIcon;
			default:
				return CogIcon;
		}
	};

	const links = $derived([
		...groups.map((g) => ({
			label: g.label.replace(/ Settings$/i, ''),
			href: `${base}/settings/${g.slug}`,
			slug: g.slug,
			icon: iconFor(g.slug)
		})),
		{
			label: 'Appearance',
			href: `${base}/settings/appearance`,
			slug: 'appearance',
			icon: PaletteIcon
		}
	]);

	const activePath = $derived($page.url.pathname);
</script>

<Navigation layout="sidebar" class="sticky top-0 h-full min-w-56 border-r border-border">
	<Navigation.Content>
		<Navigation.Group>
			<Navigation.Menu>
				<Navigation.TriggerAnchor href={`${base}/settings`}>
					<CogIcon class="size-4" />
					<Navigation.TriggerText>Settings</Navigation.TriggerText>
				</Navigation.TriggerAnchor>
			</Navigation.Menu>
		</Navigation.Group>
		<Navigation.Group>
			<Navigation.Label class="pl-2">Configuration</Navigation.Label>
			<Navigation.Menu>
				{#each links as link (link.href)}
					{@const Icon = link.icon}
					{@const active = activePath === link.href || activePath.startsWith(link.href + '/')}
					<Navigation.TriggerAnchor
						href={link.href}
						title={link.label}
						aria-label={link.label}
						aria-current={active ? 'page' : undefined}
						class={active ? 'bg-muted' : undefined}
					>
						<Icon class="size-4" />
						<Navigation.TriggerText>{link.label}</Navigation.TriggerText>
					</Navigation.TriggerAnchor>
				{/each}
			</Navigation.Menu>
		</Navigation.Group>
	</Navigation.Content>
</Navigation>

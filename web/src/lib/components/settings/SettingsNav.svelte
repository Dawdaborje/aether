<script lang="ts">
	import { page } from '$app/state';
	import { base } from '$app/paths';
	import { authSession } from '$lib/auth/session.svelte';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { iconRef } from '$lib/icons';
	import type { CatalogGroup } from '$lib/settings/api';

	let { groups = [] }: { groups?: CatalogGroup[] } = $props();

	const links = $derived([
		...groups.map((group) => ({
			label: group.label.replace(/ Settings$/i, ''),
			href: `${base}/desk/settings/${group.slug}`,
			icon: iconRef(group.icon) ?? 'settings'
		})),
		// Choosing the organization's theme is for developers (the server refuses anyone else).
		...(authSession.isDeveloper
			? [{ label: 'Appearance', href: `${base}/desk/settings/appearance`, icon: 'palette' }]
			: [])
	]);
</script>

<nav aria-label="Settings" class="md:sticky md:top-20 md:self-start">
	<ul class="flex gap-1 overflow-x-auto md:flex-col md:overflow-visible">
		{#each links as link (link.href)}
			{@const active = page.url.pathname === link.href}
			<li>
				<!-- eslint-disable svelte/no-navigation-without-resolve -- hrefs carry the app's base path -->
				<a
					href={link.href}
					aria-current={active ? 'page' : undefined}
					class="flex items-center gap-2.5 rounded-sm px-3 py-2 text-sm whitespace-nowrap transition-colors {active
						? 'bg-accent font-medium text-accent-foreground'
						: 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
				>
					<AppIcon name={link.icon} class="size-4" />
					{link.label}
				</a>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			</li>
		{/each}
	</ul>
</nav>

<script lang="ts">
	import {
		BellIcon,
		BuildingIcon,
		KeyRoundIcon,
		PaletteIcon,
		PlugIcon,
		SettingsIcon,
		ShieldIcon,
		UserIcon,
		UsersIcon
	} from '@lucide/svelte';
	import { Navigation } from '@skeletonlabs/skeleton-svelte';

	// NOTE: hrefs are placeholders until the settings sub-routes exist.
	const linksSidebar = {
		account: [
			{ label: 'Profile', href: '#', icon: UserIcon },
			{ label: 'Notifications', href: '#', icon: BellIcon },
			{ label: 'Appearance', href: '#', icon: PaletteIcon }
		],
		organization: [
			{ label: 'General', href: '#', icon: BuildingIcon },
			{ label: 'Members', href: '#', icon: UsersIcon }
		],
		security: [
			{ label: 'Security', href: '#', icon: ShieldIcon },
			{ label: 'API Keys', href: '#', icon: KeyRoundIcon },
			{ label: 'Integrations', href: '#', icon: PlugIcon }
		]
	};
</script>

<Navigation layout="sidebar" class="sticky top-0 h-full">
	<Navigation.Content>
		<Navigation.Group>
			<Navigation.Menu>
				<Navigation.TriggerAnchor href="#">
					<SettingsIcon class="size-4" />
					<Navigation.TriggerText>Settings</Navigation.TriggerText>
				</Navigation.TriggerAnchor>
			</Navigation.Menu>
		</Navigation.Group>
		{#each Object.entries(linksSidebar) as [category, links]}
			<Navigation.Group>
				<Navigation.Label class="pl-2 capitalize">{category}</Navigation.Label>
				<Navigation.Menu>
					{#each links as link (link)}
						{@const Icon = link.icon}
						<Navigation.TriggerAnchor href={link.href} title={link.label} aria-label={link.label}>
							<Icon class="size-4" />
							<Navigation.TriggerText>{link.label}</Navigation.TriggerText>
						</Navigation.TriggerAnchor>
					{/each}
				</Navigation.Menu>
			</Navigation.Group>
		{/each}
	</Navigation.Content>
</Navigation>

<script lang="ts">
	import {
		BedDoubleIcon,
		BikeIcon,
		BookIcon,
		BubblesIcon,
		HouseIcon,
		MountainIcon,
		PopcornIcon,
		SailboatIcon,
		SettingsIcon,
		SkullIcon,
		TreePalmIcon,
		TvIcon
	} from '@lucide/svelte';
	import { Navigation } from '@skeletonlabs/skeleton-svelte';

	const linksSidebar = {
		entertainment: [
			{ label: 'Books', href: '/#', icon: BookIcon },
			{ label: 'Movies', href: '/#', icon: PopcornIcon },
			{ label: 'Television', href: '/#', icon: TvIcon }
		],
		recreation: [
			{ label: 'Biking', href: '/#', icon: BikeIcon },
			{ label: 'Sailing', href: '/#', icon: SailboatIcon },
			{ label: 'Hiking', href: '/#', icon: MountainIcon }
		],
		relaxation: [
			{ label: 'Lounge', href: '/#', icon: TreePalmIcon },
			{ label: 'Spa', href: '/#', icon: BubblesIcon },
			{ label: 'Sleep', href: '/#', icon: BedDoubleIcon }
		]
	};
</script>

<!-- --- -->
<Navigation layout="sidebar" class="sticky top-0 h-full">
	<Navigation.Content>
		<Navigation.Group>
			<Navigation.Menu>
				<Navigation.TriggerAnchor href="/">
					<HouseIcon class="size-4" />
					<Navigation.TriggerText>Home</Navigation.TriggerText>
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
	<Navigation.Footer>
		<Navigation.TriggerAnchor href="/" title="Settings" aria-label="Settings">
			<SettingsIcon class="size-4" />
			<Navigation.TriggerText>Settings</Navigation.TriggerText>
		</Navigation.TriggerAnchor>
	</Navigation.Footer>
</Navigation>

<script lang="ts">
	import DefaultLayout from '$lib/components/pages/layout/default/system/defaultLayout.svelte';
	import type { NavLink } from '$lib/types/layout';

	let { children } = $props();
	type LayoutType = 'default' | 'custom';
	let layoutType: LayoutType = 'default';

	let navData: NavLink[] | null = $state([
		{
			title: 'Apps',
			items: [
				{ title: 'Home', url: '/', icon: 'home', isActive: true },
				{ title: 'About', url: '/about', icon: 'info', isActive: false }
			]
		},
		{
			title: 'Group 2',
			items: [
				{ title: 'Settings', url: '/settings', icon: 'settings', isActive: false },
				{ title: 'Profile', url: '/profile', icon: 'user', isActive: false }
			]
		}
	]);
</script>

<div class="sticky top-0 flex h-full">
	{#if layoutType === 'default'}
		<DefaultLayout {navData}>{@render children()}</DefaultLayout>
	{:else}
		{@render children()}
	{/if}
</div>

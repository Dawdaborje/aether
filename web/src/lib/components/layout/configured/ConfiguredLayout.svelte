<script lang="ts">
	import { authSession } from '$lib/auth/session.svelte';
	import { themeStore } from '$lib/theme';
	import type { LayoutDef } from '$lib/theme/types';
	import { builtInNav, deskNav } from '../nav';
	import BareLayout from '../bare/bareLayout.svelte';
	import CustomLayout from '../custom/customLayout.svelte';
	import EnterpriseShell from '../shell/EnterpriseShell.svelte';

	let { children, def }: { children: import('svelte').Snippet; def: LayoutDef } = $props();

	const chrome = $derived(def.chrome || 'top');
	const nav = $derived.by(() => {
		if (chrome === 'sidebar') return deskNav;
		return themeStore.theme.nav ?? builtInNav(authSession.isDeveloper);
	});
</script>

{#if chrome === 'bare'}
	<BareLayout>
		{@render children()}
	</BareLayout>
{:else if chrome === 'custom'}
	<CustomLayout>
		{@render children()}
	</CustomLayout>
{:else}
	<!-- top and sidebar both use the enterprise shell; sidebar uses desk nav -->
	<EnterpriseShell {nav}>
		{@render children()}
	</EnterpriseShell>
{/if}

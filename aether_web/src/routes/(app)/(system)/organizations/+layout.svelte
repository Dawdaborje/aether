<script lang="ts">
	import DefaultLayout from '$lib/components/pages/layout/default/defaultLayout.svelte';

	let { children } = $props();
	type LayoutType = 'default' | 'custom';
	let layoutType: LayoutType = 'default';
</script>

{#if layoutType === 'default'}
	<DefaultLayout>{@render children()}</DefaultLayout>
{:else}
	{@render children()}
{/if}

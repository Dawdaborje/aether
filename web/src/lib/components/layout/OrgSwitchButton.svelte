<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { tone = 'light' }: { tone?: 'light' | 'on-primary' } = $props();
</script>

<!-- Shown only where the user can actually move between organizations. -->
{#if orgStore.canSwitch}
	<Button
		type="button"
		size="sm"
		variant={tone === 'on-primary' ? 'ghost' : 'outline'}
		class={tone === 'on-primary'
			? 'border border-sidebar-border text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'
			: undefined}
		onclick={() => orgStore.openSwitcher()}
		aria-label="Switch organization"
	>
		{orgStore.currentName ?? 'Choose organization'} ▾
	</Button>
{/if}

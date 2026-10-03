<script lang="ts">
	import { orgStore } from '$lib/org/orgStore.svelte';

	let { tone = 'light' }: { tone?: 'light' | 'on-primary' } = $props();
</script>

<!-- Shown only where the user can actually move between organizations. -->
{#if orgStore.canSwitch}
	<button
		type="button"
		class="rounded-sm border px-3 py-1.5 text-sm {tone === 'on-primary'
			? 'border-white/40 text-white hover:bg-white/10'
			: 'border-border bg-card text-foreground hover:bg-accent'}"
		onclick={() => orgStore.openSwitcher()}
		aria-label="Switch organization"
	>
		{orgStore.currentName ?? 'Choose organization'} ▾
	</button>
{/if}

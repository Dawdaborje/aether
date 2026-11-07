<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { getShellState } from '../shellContext.svelte';

	const shell = getShellState();
	const label = $derived(shell.sidebarCollapsed ? 'Show the sidebar' : 'Hide the sidebar');
</script>

<button
	type="button"
	data-part="shell.sidebar-toggle"
	aria-label={label}
	aria-pressed={shell.sidebarCollapsed}
	title={label}
	onclick={() => shell.toggleSidebar()}
	class="flex size-9 items-center justify-center rounded-md text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
>
	<AppIcon name="side-panel-open" class="size-5" />
</button>

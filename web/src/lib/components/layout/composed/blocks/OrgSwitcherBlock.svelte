<script lang="ts">
	import { orgStore } from '$lib/org/orgStore.svelte';
</script>

<!-- The installation's organization. (The block keeps its `org-switcher` name: themes use it.) -->
{#if orgStore.currentName}
	<span
		aria-label="Organization"
		class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-region-muted-foreground"
	>
		<span class="size-1.5 rounded-full bg-region-primary" aria-hidden="true"></span>
		<span class="max-w-40 truncate">{orgStore.currentName}</span>
	</span>
{/if}

<script lang="ts">
	import type { OrgRole } from '$lib/org/api';
	import RoleChip from './RoleChip.svelte';

	let {
		roles = [],
		catalog = null,
		maxVisible = 3
	}: {
		roles?: string[];
		catalog?: OrgRole[] | null;
		maxVisible?: number;
	} = $props();

	const visible = $derived(roles.slice(0, maxVisible));
	const rest = $derived(Math.max(0, roles.length - maxVisible));
</script>

{#if roles.length === 0}
	<span class="text-muted-foreground">—</span>
{:else}
	<span class="flex flex-wrap items-center gap-1">
		{#each visible as role (role)}
			<RoleChip name={role} {catalog} />
		{/each}
		{#if rest > 0}
			<span
				class="rounded-sm bg-muted px-1.5 py-0.5 text-xs text-muted-foreground"
				title={roles.slice(maxVisible).join(', ')}>+{rest}</span
			>
		{/if}
	</span>
{/if}

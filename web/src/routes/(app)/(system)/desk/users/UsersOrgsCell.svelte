<script lang="ts">
	import { resolve } from '$app/paths';

	let { organizations }: { organizations: string[] } = $props();
</script>

{#if organizations.length === 0}
	<span class="text-muted-foreground">—</span>
{:else}
	<span class="flex flex-wrap gap-1">
		{#each organizations as org (org)}
			<a
				class="rounded-sm bg-muted px-1.5 py-0.5 font-mono text-xs hover:bg-accent"
				href={resolve(`/desk/organizations/${org}/members`)}
				onclick={(event) => event.stopPropagation()}
			>
				{org}
			</a>
		{/each}
	</span>
{/if}

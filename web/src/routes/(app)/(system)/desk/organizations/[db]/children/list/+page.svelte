<script lang="ts">
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { Badge } from '$lib/components/ui/badge';
	import { Empty } from '$lib/components/ui/empty';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { descendantsOf, flatten } from '$lib/org/hierarchy';
	import type { OrganizationProfile } from '$lib/org/api';

	let { params }: { params: { db: string } } = $props();
	const ctx = getContext<{ organizations: OrganizationProfile[] | null; failed: string | null }>(
		'org-children'
	);

	const rows = $derived(
		ctx.organizations ? flatten(descendantsOf(ctx.organizations, params.db)) : null
	);
</script>

{#if rows === null}
	<div class="space-y-2">
		{#each Array(3) as _, i (i)}
			<Skeleton class="h-12 w-full rounded-lg" />
		{/each}
	</div>
{:else if ctx.failed}
	<LoadFailed message={ctx.failed} />
{:else if rows.length === 0}
	<Empty
		icon="network"
		title="No child organizations"
		description="Organizations created with this one as their parent show up here."
	/>
{:else}
	<ul class="divide-y divide-border rounded-lg border border-border">
		{#each rows as { org, depth } (org.db_name)}
			<li
				class="flex flex-wrap items-center gap-3 px-4 py-3"
				style:padding-left="{depth * 1.25}rem"
			>
				<div class="min-w-0 flex-1">
					<a
						class="text-sm font-medium hover:underline"
						href={resolve(`/desk/organizations/${org.db_name}/overview`)}>{org.name}</a
					>
					<p class="font-mono text-xs text-muted-foreground">{org.db_name}</p>
				</div>
				{#if depth > 1}<Badge variant="outline">via {org.parent_name ?? org.parent_db_name}</Badge
					>{/if}
				<span class="text-xs text-muted-foreground">{org.member_count} member(s)</span>
			</li>
		{/each}
	</ul>
	<p class="text-xs text-muted-foreground">{rows.length} below this organization</p>
{/if}

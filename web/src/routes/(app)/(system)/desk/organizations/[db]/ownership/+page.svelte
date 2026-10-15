<script lang="ts">
	import { onMount } from 'svelte';
	import HistoryIcon from '@lucide/svelte/icons/history';
	import { Empty } from '$lib/components/ui/empty';
	import { fetchOwnershipTransfers, type OwnershipTransfer } from '$lib/org/api';
	import { getOrgDetailContext } from '$lib/org/orgDetailContext';

	let { params }: { params: { db: string } } = $props();

	const detail = getOrgDetailContext();
	let transfers = $state<OwnershipTransfer[] | null>(null);
	let failed = $state<string | null>(null);

	onMount(async () => {
		try {
			transfers = await fetchOwnershipTransfers(params.db);
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Ownership history could not be loaded.';
			transfers = [];
		}
	});
</script>

<div class="space-y-4">
	<p class="text-sm text-muted-foreground">
		Current owner:
		<span class="font-medium text-foreground"
			>{detail.organization?.owner_login ?? 'unknown'}</span
		>. Ownership can be transferred with
		<code class="rounded bg-muted px-1 py-0.5 text-xs">aether --transfer-org-ownership</code>
		or
		<code class="rounded bg-muted px-1 py-0.5 text-xs">POST /api/ui/organizations/transfer-ownership</code>.
		Admins are separate: grant
		<code class="rounded bg-muted px-1 py-0.5 text-xs">org_admin</code>
		to other members without changing the owner.
	</p>

	{#if transfers === null}
		<p class="text-sm text-muted-foreground">Loading history…</p>
	{:else if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{:else if transfers.length === 0}
		<Empty
			icon={HistoryIcon}
			title="No transfers yet"
			description="The first member remains the owner until ownership is transferred."
		/>
	{:else}
		<ul class="divide-y divide-border rounded-lg border border-border">
			{#each transfers as row, i (i)}
				<li class="space-y-1 px-4 py-3">
					<p class="text-sm">
						<span class="font-medium">{row.from_login ?? row.from_user_id}</span>
						→
						<span class="font-medium">{row.to_login ?? row.to_user_id}</span>
					</p>
					<p class="text-xs text-muted-foreground">
						{row.date_created}
						{#if row.note}
							· {row.note}
						{/if}
					</p>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<script lang="ts">
	import ConfirmDialog from '$lib/components/dialogs/ConfirmDialog.svelte';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect, NativeSelectOption } from '$lib/components/ui/native-select';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { errorMessage } from '$lib/errors/report';
	import { fetchOrganizationMembers, transferOwnership, type OrgMember } from '$lib/org/api';

	/**
	 * Make a member the owner of an organization. With `member` the new owner is fixed (the
	 * member's own page); without it the dialog lists the organization's active members to
	 * choose from. The previous owner keeps `org_admin`.
	 */
	let {
		open = $bindable(false),
		db,
		organizationName = db,
		member = null,
		onDone
	}: {
		open?: boolean;
		db: string;
		organizationName?: string;
		member?: OrgMember | null;
		onDone?: () => void | Promise<void>;
	} = $props();

	let candidates = $state<OrgMember[] | null>(null);
	let chosen = $state('');
	let note = $state('');
	let working = $state(false);
	let failed = $state<string | null>(null);

	function nameOf(person: OrgMember): string {
		return person.display_name || person.username || person.email || person.user_id;
	}

	$effect(() => {
		if (!open) return;
		note = '';
		failed = null;
		if (member) {
			chosen = member.user_id;
			return;
		}
		chosen = '';
		candidates = null;
		fetchOrganizationMembers(db)
			.then((members) => {
				candidates = members.filter((person) => !person.is_owner && person.is_active !== false);
			})
			.catch((error) => {
				failed = errorMessage(error, 'Members could not be loaded.');
				candidates = [];
			});
	});

	const target = $derived(
		member ?? candidates?.find((person) => person.user_id === chosen) ?? null
	);

	async function confirm() {
		if (!target) return;
		working = true;
		failed = null;
		try {
			await transferOwnership(db, target.user_id, note);
			open = false;
			await onDone?.();
		} catch (error) {
			failed = errorMessage(error, 'Ownership could not be transferred.');
		} finally {
			working = false;
		}
	}
</script>

<ConfirmDialog
	bind:open
	title={member ? `Make ${nameOf(member)} the owner?` : 'Transfer ownership'}
	description={`${organizationName} gets a new owner. The current owner stays an admin (org_admin), and the transfer is kept in the ownership history.`}
	confirmLabel="Make owner"
	busy={working}
	disabled={!target}
	error={failed}
	onConfirm={confirm}
>
	{#if !member}
		{#if candidates === null}
			<Skeleton class="h-9 w-full rounded-md" />
		{:else if candidates.length === 0 && !failed}
			<p class="text-sm text-muted-foreground">
				There is no other active member to hand the organization to. Add the new owner as a member
				first.
			</p>
		{:else if candidates.length > 0}
			<label class="block space-y-1 text-sm">
				<span class="text-muted-foreground">New owner</span>
				<NativeSelect
					class="w-full"
					value={chosen}
					onchange={(event) => (chosen = event.currentTarget.value)}
					disabled={working}
				>
					<NativeSelectOption value="">Choose a member…</NativeSelectOption>
					{#each candidates as person (person.user_id)}
						<NativeSelectOption value={person.user_id}>
							{nameOf(person)}{person.email && person.email !== nameOf(person)
								? ` (${person.email})`
								: ''}
						</NativeSelectOption>
					{/each}
				</NativeSelect>
			</label>
		{/if}
	{/if}
	<label class="block space-y-1 text-sm">
		<span class="text-muted-foreground">Note (optional)</span>
		<Input bind:value={note} placeholder="Why the organization changes hands" disabled={working} />
	</label>
</ConfirmDialog>

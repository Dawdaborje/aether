<script lang="ts">
	import { TableSkeleton } from '$lib/components/ui/skeleton';
	import { page } from '$app/state';
	import { textParam } from '$lib/url/filterParams';
	import { syncFilters } from '$lib/url/syncFilters.svelte';
	import { reportError } from '$lib/errors/report';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ColumnDef, RowSelectionState } from '@tanstack/table-core';
	import { getCoreRowModel, getPaginationRowModel } from '@tanstack/table-core';
	import { Button } from '$lib/components/ui/button';
	import {
		createSvelteTable,
		DataTablePagination,
		DataTable,
		DataTableSelected,
		renderComponent,
		selectColumn
	} from '$lib/components/ui/data-table';
	import DataTableBadgeCell from '$lib/components/ui/data-table/data-table-badge-cell.svelte';
	import { Empty } from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import {
		addOrganizationMember,
		fetchOrganizationMembers,
		fetchOrganizationRoles,
		fetchOwnershipTransfers,
		inviteOrganizationMember,
		type OrgMember,
		type OrgRole,
		type OwnershipTransfer
	} from '$lib/org/api';
	import TransferOwnershipDialog from './TransferOwnershipDialog.svelte';
	import MemberNameCell from './MemberNameCell.svelte';
	import MemberRolesCell from './MemberRolesCell.svelte';

	let {
		db,
		title = 'Members',
		/**
		 * Where row clicks go.
		 * - `desk` (default): `/desk/users/{id}?org={db}` — full page outside org tabs
		 * - `org`: `/users/{id}` — non-desk managers
		 */
		detailMode = 'desk',
		/** Show the owner, the transfer action and the transfer history (desk only). */
		ownership = false
	}: {
		db: string;
		title?: string;
		detailMode?: 'desk' | 'org';
		ownership?: boolean;
	} = $props();

	let members = $state<OrgMember[] | null>(null);
	let roleCatalog = $state<OrgRole[]>([]);
	let failed = $state<string | null>(null);
	let working = $state(false);
	let mode = $state<'invite' | 'existing'>('invite');
	let login = $state('');
	let username = $state('');
	let email = $state('');
	let query = $state(textParam(page.url.searchParams, 'q'));
	syncFilters(() => ({ q: query }));
	let inviteNotice = $state<{
		password: string;
		username: string;
		email_sent: boolean;
		email_error?: string | null;
	} | null>(null);

	let transfers = $state<OwnershipTransfer[]>([]);
	let transferring = $state(false);
	let ownerNotice = $state<string | null>(null);
	const owner = $derived((members ?? []).find((member) => member.is_owner));

	async function loadTransfers() {
		if (!ownership) return;
		try {
			transfers = await fetchOwnershipTransfers(db);
		} catch (error) {
			failed = reportError(error, 'Ownership history could not be loaded.');
		}
	}

	async function transferred() {
		ownerNotice = 'Ownership transferred.';
		await Promise.all([reload(), loadTransfers()]);
	}

	let rowSelection = $state<RowSelectionState>({});

	const visible = $derived(
		(members ?? []).filter((member) => {
			const needle = query.trim().toLowerCase();
			if (!needle) return true;
			return [
				member.display_name,
				member.username,
				member.email,
				member.user_id,
				...(member.roles ?? [])
			]
				.filter(Boolean)
				.join(' ')
				.toLowerCase()
				.includes(needle);
		})
	);

	$effect(() => {
		void visible;
		rowSelection = {};
	});

	function displayName(member: OrgMember): string {
		return member.display_name || member.username || member.email || member.user_id;
	}

	const columns = $derived.by((): ColumnDef<OrgMember>[] => [
		selectColumn<OrgMember>(),
		{
			id: 'name',
			header: 'Name',
			cell: ({ row }) =>
				renderComponent(MemberNameCell, {
					name: displayName(row.original),
					isOwner: row.original.is_owner
				})
		},
		{
			accessorKey: 'username',
			header: 'Username',
			cell: ({ row }) => row.original.username || '—'
		},
		{
			accessorKey: 'email',
			header: 'Email',
			cell: ({ row }) => row.original.email || '—'
		},
		{
			id: 'roles',
			header: 'Roles',
			cell: ({ row }) =>
				renderComponent(MemberRolesCell, {
					roles: row.original.roles ?? [],
					catalog: roleCatalog
				})
		},
		{
			id: 'status',
			header: 'Status',
			cell: ({ row }) =>
				row.original.is_active === false
					? renderComponent(DataTableBadgeCell, { label: 'Inactive', variant: 'secondary' })
					: 'Active'
		},
		{
			id: 'joined',
			header: 'Joined',
			cell: ({ row }) => row.original.date_joined || '—'
		}
	]);

	const table = createSvelteTable({
		get data() {
			return visible;
		},
		get columns() {
			return columns;
		},
		state: {
			get rowSelection() {
				return rowSelection;
			}
		},
		onRowSelectionChange: (updater) => {
			rowSelection = typeof updater === 'function' ? updater(rowSelection) : updater;
		},
		getCoreRowModel: getCoreRowModel(),
		getPaginationRowModel: getPaginationRowModel(),
		enableRowSelection: true,
		getRowId: (row) => row.user_id
	});

	const selectedCount = $derived(
		Object.keys(rowSelection).filter((key) => rowSelection[key]).length
	);

	async function reload() {
		failed = null;
		try {
			const [nextMembers, nextRoles] = await Promise.all([
				fetchOrganizationMembers(db),
				fetchOrganizationRoles(db).catch(() => [] as OrgRole[])
			]);
			members = nextMembers;
			roleCatalog = nextRoles;
		} catch (error) {
			failed = reportError(error, 'Members could not be loaded.');
			members = members ?? [];
		}
	}

	$effect(() => {
		void db;
		void reload();
		void loadTransfers();
	});

	async function addMember() {
		working = true;
		failed = null;
		inviteNotice = null;
		try {
			if (mode === 'invite') {
				const result = await inviteOrganizationMember(db, {
					email: email.trim(),
					username: username.trim() || undefined
				});
				members = result.members;
				if (result.temporary_password) {
					inviteNotice = {
						password: result.temporary_password,
						username: result.username ?? email.trim(),
						email_sent: result.email_sent,
						email_error: result.email_error
					};
				}
				username = '';
				email = '';
			} else {
				members = (await addOrganizationMember(db, {
					mode: 'existing',
					login: login.trim()
				})) as OrgMember[];
				login = '';
			}
		} catch (error) {
			failed = reportError(error, 'Member could not be added.');
		} finally {
			working = false;
		}
	}

	function openMember(member: OrgMember) {
		const id = encodeURIComponent(member.user_id);
		if (detailMode === 'org') {
			void goto(resolve(`/users/${id}`));
			return;
		}
		void goto(`${resolve(`/desk/users/${id}`)}?org=${encodeURIComponent(db)}`);
	}
</script>

<section class="flex min-h-0 w-full flex-1 flex-col gap-5">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h2 class="text-lg font-semibold tracking-tight">{title}</h2>
			<p class="text-sm text-muted-foreground">
				Invite people, manage roles, and open a member for password or access changes.
			</p>
		</div>
	</div>

	{#if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{/if}

	{#if ownership}
		<div class="space-y-3 rounded-md border border-border bg-card p-4">
			<div class="flex flex-wrap items-start justify-between gap-3">
				<p class="max-w-3xl min-w-0 flex-1 text-sm text-muted-foreground">
					Owner:
					<span class="font-medium text-foreground">{owner ? displayName(owner) : 'unknown'}</span>.
					The owner can be any member; the previous owner stays an admin. Admins are separate: grant
					<code class="rounded bg-muted px-1 py-0.5 text-xs">org_admin</code> to other members without
					changing the owner.
				</p>
				<Button size="sm" onclick={() => (transferring = true)}>Transfer ownership</Button>
			</div>
			{#if ownerNotice}
				<p class="text-sm text-muted-foreground" role="status">{ownerNotice}</p>
			{/if}
			{#if transfers.length > 0}
				<details class="text-sm">
					<summary class="cursor-pointer text-muted-foreground">
						Ownership history ({transfers.length})
					</summary>
					<ul class="mt-2 divide-y divide-border rounded-md border border-border">
						{#each transfers as row, i (i)}
							<li class="space-y-1 px-3 py-2">
								<p>
									<span class="font-medium">{row.from_login ?? row.from_user_id}</span>
									→
									<span class="font-medium">{row.to_login ?? row.to_user_id}</span>
								</p>
								<p class="text-xs text-muted-foreground">
									{row.date_created}{row.note ? ` · ${row.note}` : ''}
								</p>
							</li>
						{/each}
					</ul>
				</details>
			{/if}
		</div>
	{/if}

	{#if inviteNotice}
		<div class="space-y-2 rounded-md border border-border bg-muted/40 p-4 text-sm" role="status">
			<p class="font-medium">
				{#if inviteNotice.email_sent}
					Invite emailed to the new user.
				{:else}
					Account created{inviteNotice.email_error
						? ` (email not sent: ${inviteNotice.email_error})`
						: ''}. Copy the temporary password now.
				{/if}
			</p>
			<p class="font-mono text-xs">
				Username: {inviteNotice.username}<br />
				Temporary password: {inviteNotice.password}
			</p>
			<Button size="sm" variant="ghost" onclick={() => (inviteNotice = null)}>Dismiss</Button>
		</div>
	{/if}

	<div class="space-y-3 rounded-md border border-border bg-card p-4">
		<div class="flex flex-wrap gap-2">
			<button
				type="button"
				class="rounded-md px-2.5 py-1 text-sm {mode === 'invite'
					? 'bg-accent font-medium'
					: 'text-muted-foreground hover:bg-muted'}"
				onclick={() => (mode = 'invite')}
			>
				Invite
			</button>
			<button
				type="button"
				class="rounded-md px-2.5 py-1 text-sm {mode === 'existing'
					? 'bg-accent font-medium'
					: 'text-muted-foreground hover:bg-muted'}"
				onclick={() => (mode = 'existing')}
			>
				Existing user
			</button>
		</div>
		{#if mode === 'existing'}
			<div class="flex flex-wrap items-end gap-2">
				<label class="min-w-[14rem] flex-1 space-y-1 text-sm">
					<span class="text-muted-foreground">Username or email</span>
					<Input bind:value={login} autocomplete="off" />
				</label>
				<Button size="sm" disabled={working || !login.trim()} onclick={addMember}>
					Add to organization
				</Button>
			</div>
		{:else}
			<div class="grid gap-2 sm:grid-cols-2">
				<label class="space-y-1 text-sm">
					<span class="text-muted-foreground">Email</span>
					<Input bind:value={email} type="email" autocomplete="off" />
				</label>
				<label class="space-y-1 text-sm">
					<span class="text-muted-foreground">Username (optional)</span>
					<Input bind:value={username} autocomplete="off" />
				</label>
			</div>
			<p class="text-xs text-muted-foreground">
				A temporary password is generated and emailed (plain + HTML). You will also see it once here
				if email is not configured.
			</p>
			<Button size="sm" disabled={working || !email.trim()} onclick={addMember}>Send invite</Button>
		{/if}
	</div>

	{#if members === null}
		<TableSkeleton columns={4} label="Loading members…" />
	{:else if members.length === 0}
		<Empty
			icon="users"
			title="No members"
			description="Invite someone or add an existing account above."
		/>
	{:else}
		<div class="flex flex-wrap items-center justify-between gap-3">
			<p class="text-xs text-muted-foreground">
				{visible.length} of {members.length} members · click a row to open details
			</p>
			<label class="relative block w-full max-w-xs">
				<input
					type="search"
					placeholder="Filter members"
					aria-label="Filter members"
					class="h-9 w-full rounded-sm border border-input bg-card px-3 text-sm"
					bind:value={query}
				/>
			</label>
		</div>
		{#if visible.length === 0}
			<p class="text-sm text-muted-foreground">Nothing matches “{query.trim()}”.</p>
		{:else}
			<div class="space-y-2">
				<DataTableSelected count={selectedCount} onClear={() => (rowSelection = {})} />
				<DataTable {table} onRowClick={openMember} />
				<DataTablePagination {table} />
			</div>
		{/if}
	{/if}
</section>

{#if ownership}
	<TransferOwnershipDialog
		bind:open={transferring}
		{db}
		organizationName={db}
		onDone={transferred}
	/>
{/if}

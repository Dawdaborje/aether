<script lang="ts">
	import { TableSkeleton } from '$lib/components/ui/skeleton';
	import { page } from '$app/state';
	import { textParam } from '$lib/url/filterParams';
	import { syncFilters } from '$lib/url/syncFilters.svelte';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { loadFailure, reportError } from '$lib/errors/report';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ColumnDef, RowSelectionState } from '@tanstack/table-core';
	import { getCoreRowModel, getPaginationRowModel } from '@tanstack/table-core';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import {
		createSvelteTable,
		DataTable,
		DataTablePagination,
		DataTableSelected,
		renderComponent,
		selectColumn
	} from '$lib/components/ui/data-table';
	import DataTableBadgeCell from '$lib/components/ui/data-table/data-table-badge-cell.svelte';
	import { Empty } from '$lib/components/ui/empty';
	import { createPlatformUser, fetchPlatformUsers, type PlatformUser } from '$lib/org/api';
	import UsersOrgsCell from './UsersOrgsCell.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let users = $state<PlatformUser[] | null>(null);
	let failed = $state<string | null>(null);
	let query = $state(textParam(page.url.searchParams, 'q'));
	syncFilters(() => ({ q: query }));
	let rowSelection = $state<RowSelectionState>({});

	let creating = $state(false);
	let saving = $state(false);
	let formError = $state<string | null>(null);
	let newUsername = $state('');
	let newEmail = $state('');
	let newPassword = $state('');

	async function submitNewUser(event: SubmitEvent) {
		event.preventDefault();
		formError = null;
		saving = true;
		try {
			users = await createPlatformUser({
				username: newUsername.trim(),
				email: newEmail.trim(),
				password: newPassword
			});
			newUsername = '';
			newEmail = '';
			newPassword = '';
			creating = false;
			failed = null;
		} catch (error) {
			reportError(error, 'User could not be created.');
		} finally {
			saving = false;
		}
	}

	const visible = $derived(
		(users ?? []).filter((user) => {
			const needle = query.trim().toLowerCase();
			if (!needle) return true;
			return [
				user.username,
				user.email,
				user.display_name,
				user.user_id,
				...(orgStore.isSingle ? [] : user.organizations)
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

	function displayName(user: PlatformUser): string {
		return user.display_name || user.username || user.email || user.user_id;
	}

	function openUser(user: PlatformUser) {
		// One organization: every account is in it, whatever the list says.
		const org = orgStore.isSingle ? orgStore.current : user.organizations[0];
		if (!org) return;
		const id = encodeURIComponent(user.user_id);
		const path = resolve(`/desk/users/${id}`);
		void goto(`${path}?org=${encodeURIComponent(org)}`);
	}

	const columns = $derived.by((): ColumnDef<PlatformUser>[] => [
		selectColumn<PlatformUser>(),
		{
			id: 'name',
			accessorFn: (row) => displayName(row),
			header: 'Name',
			cell: ({ row }) => displayName(row.original)
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
		...(orgStore.isSingle
			? []
			: [
					{
						id: 'organizations',
						header: 'Organizations',
						cell: ({ row }) =>
							renderComponent(UsersOrgsCell, { organizations: row.original.organizations })
					} satisfies ColumnDef<PlatformUser>
				]),
		{
			id: 'status',
			header: 'Status',
			cell: ({ row }) =>
				row.original.is_active
					? 'Active'
					: renderComponent(DataTableBadgeCell, { label: 'Inactive', variant: 'secondary' })
		},
		{
			id: 'flags',
			header: '',
			cell: ({ row }) =>
				row.original.is_developer ? renderComponent(DataTableBadgeCell, { label: 'Developer' }) : ''
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

	async function load() {
		users = null;
		failed = null;
		try {
			users = await fetchPlatformUsers();
		} catch (error) {
			failed = loadFailure(error, 'Users could not be loaded.');
			users = [];
		}
	}

	onMount(load);
</script>

<ControlPanel crumbs={[{ label: 'Users' }]} />

<div class="w-full space-y-4 px-4 py-4 md:px-6">
	<header class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h1 class="text-xl font-semibold tracking-tight">Users</h1>
			<p class="text-sm text-muted-foreground">
				{#if orgStore.isSingle}
					Every account on this instance. Open a row to manage roles and passwords.
				{:else}
					Every account on this instance. Open a row’s organization to manage membership, roles, and
					passwords.
				{/if}
			</p>
		</div>
		<div class="flex w-full max-w-md items-center gap-2">
			<label class="relative block w-full">
				<input
					type="search"
					placeholder="Filter users"
					aria-label="Filter users"
					class="h-9 w-full rounded-sm border border-input bg-card px-3 text-sm"
					bind:value={query}
				/>
			</label>
			<button
				type="button"
				class="h-9 shrink-0 rounded-sm bg-primary px-3 text-sm font-medium text-primary-foreground"
				onclick={() => (creating = !creating)}
			>
				{creating ? 'Cancel' : 'New user'}
			</button>
		</div>
	</header>

	{#if creating}
		<form
			class="grid gap-3 rounded-sm border border-input bg-card p-4 sm:grid-cols-3"
			onsubmit={submitNewUser}
		>
			<label class="block text-sm">
				<span class="mb-1 block text-muted-foreground">Username</span>
				<input
					class="h-9 w-full rounded-sm border border-input bg-background px-3 text-sm"
					required
					minlength="3"
					autocomplete="off"
					bind:value={newUsername}
				/>
			</label>
			<label class="block text-sm">
				<span class="mb-1 block text-muted-foreground">Email</span>
				<input
					type="email"
					class="h-9 w-full rounded-sm border border-input bg-background px-3 text-sm"
					required
					autocomplete="off"
					bind:value={newEmail}
				/>
			</label>
			<label class="block text-sm">
				<span class="mb-1 block text-muted-foreground">Password (8–128 characters)</span>
				<input
					type="password"
					class="h-9 w-full rounded-sm border border-input bg-background px-3 text-sm"
					required
					minlength="8"
					maxlength="128"
					autocomplete="new-password"
					bind:value={newPassword}
				/>
			</label>
			<div class="flex items-center gap-3 sm:col-span-3">
				<button
					type="submit"
					disabled={saving}
					class="h-9 rounded-sm bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-60"
				>
					{saving ? 'Creating…' : 'Create user'}
				</button>
				{#if formError}
					<p class="text-sm text-destructive" role="alert">{formError}</p>
				{/if}
			</div>
		</form>
	{/if}

	{#if users === null}
		<TableSkeleton columns={4} label="Loading users…" />
	{:else if failed}
		<LoadFailed message={failed} onRetry={load} />
	{:else if users.length === 0}
		<Empty
			icon="users"
			title="No users"
			description="Use New user above, or add one from an organization."
		/>
	{:else if visible.length === 0}
		<p class="text-sm text-muted-foreground">Nothing matches “{query.trim()}”.</p>
	{:else}
		<p class="text-xs text-muted-foreground">{visible.length} of {users.length} users</p>
		<DataTableSelected count={selectedCount} onClear={() => (rowSelection = {})} />
		<DataTable
			{table}
			onRowClick={openUser}
			rowClass={(user) =>
				orgStore.isSingle || user.organizations.length > 0 ? 'cursor-pointer' : undefined}
		/>
		<DataTablePagination {table} />
	{/if}
</div>

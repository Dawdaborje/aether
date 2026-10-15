<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ColumnDef, RowSelectionState } from '@tanstack/table-core';
	import { getCoreRowModel } from '@tanstack/table-core';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import {
		createSvelteTable,
		DataTable,
		DataTableSelected,
		renderComponent,
		selectColumn
	} from '$lib/components/ui/data-table';
	import DataTableBadgeCell from '$lib/components/ui/data-table/data-table-badge-cell.svelte';
	import { Empty } from '$lib/components/ui/empty';
	import UsersIcon from '@lucide/svelte/icons/users';
	import { fetchPlatformUsers, type PlatformUser } from '$lib/org/api';
	import UsersOrgsCell from './UsersOrgsCell.svelte';

	let users = $state<PlatformUser[] | null>(null);
	let failed = $state<string | null>(null);
	let query = $state('');
	let rowSelection = $state<RowSelectionState>({});

	const visible = $derived(
		(users ?? []).filter((user) => {
			const needle = query.trim().toLowerCase();
			if (!needle) return true;
			return [
				user.username,
				user.email,
				user.display_name,
				user.user_id,
				...user.organizations
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
		if (user.organizations.length === 0) return;
		const id = encodeURIComponent(user.user_id);
		const path = resolve(`/desk/users/${id}`);
		const org = user.organizations[0];
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
		{
			id: 'organizations',
			header: 'Organizations',
			cell: ({ row }) =>
				renderComponent(UsersOrgsCell, { organizations: row.original.organizations })
		},
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
				row.original.is_developer
					? renderComponent(DataTableBadgeCell, { label: 'Developer' })
					: ''
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
		enableRowSelection: true,
		getRowId: (row) => row.user_id
	});

	const selectedCount = $derived(Object.keys(rowSelection).filter((key) => rowSelection[key]).length);

	onMount(async () => {
		try {
			users = await fetchPlatformUsers();
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Users could not be loaded.';
			users = [];
		}
	});
</script>

<ControlPanel crumbs={[{ label: 'Users' }]} />

<div class="w-full space-y-4 px-4 py-4 md:px-6">
	<header class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h1 class="text-xl font-semibold tracking-tight">Users</h1>
			<p class="text-sm text-muted-foreground">
				Every account on this instance. Open a row’s organization to manage membership, roles, and
				passwords.
			</p>
		</div>
		<label class="relative block w-full max-w-xs">
			<input
				type="search"
				placeholder="Filter users"
				aria-label="Filter users"
				class="h-9 w-full rounded-sm border border-input bg-card px-3 text-sm"
				bind:value={query}
			/>
		</label>
	</header>

	{#if users === null}
		<p class="text-sm text-muted-foreground">Loading users…</p>
	{:else if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{:else if users.length === 0}
		<Empty icon={UsersIcon} title="No users" description="Create a user from an organization or the CLI." />
	{:else if visible.length === 0}
		<p class="text-sm text-muted-foreground">Nothing matches “{query.trim()}”.</p>
	{:else}
		<p class="text-xs text-muted-foreground">{visible.length} of {users.length} users</p>
		<DataTableSelected count={selectedCount} onClear={() => (rowSelection = {})} />
		<DataTable
			{table}
			onRowClick={openUser}
			rowClass={(user) => (user.organizations.length > 0 ? 'cursor-pointer' : undefined)}
		/>
	{/if}
</div>

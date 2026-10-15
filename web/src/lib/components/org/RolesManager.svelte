<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ColumnDef } from '@tanstack/table-core';
	import { getCoreRowModel } from '@tanstack/table-core';
	import { Button } from '$lib/components/ui/button';
	import { createSvelteTable, DataTable, renderComponent } from '$lib/components/ui/data-table';
	import DataTableBadgeCell from '$lib/components/ui/data-table/data-table-badge-cell.svelte';
	import { Empty } from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import {
		createOrganizationRole,
		fetchOrganizationRoles,
		type OrgRole
	} from '$lib/org/api';
	import { kindLabel, roleKind } from './roleDisplay';

	let {
		db,
		title = 'Roles',
		detailMode = 'desk'
	}: {
		db: string;
		title?: string;
		detailMode?: 'desk' | 'org';
	} = $props();

	let roles = $state<OrgRole[] | null>(null);
	let failed = $state<string | null>(null);
	let working = $state(false);
	let query = $state('');
	let creating = $state(false);
	let slug = $state('');
	let label = $state('');
	let description = $state('');
	let createError = $state<string | null>(null);

	const visible = $derived(
		(roles ?? []).filter((role) => {
			const needle = query.trim().toLowerCase();
			if (!needle) return true;
			return [role.name, role.label, role.description ?? '', kindLabel(roleKind(role.name, role.is_system))]
				.join(' ')
				.toLowerCase()
				.includes(needle);
		})
	);

	const columns = $derived.by((): ColumnDef<OrgRole>[] => [
		{
			id: 'label',
			header: 'Role',
			cell: ({ row }) => row.original.label
		},
		{
			id: 'kind',
			header: 'Kind',
			cell: ({ row }) => {
				const kind = roleKind(row.original.name, row.original.is_system);
				return renderComponent(DataTableBadgeCell, {
					label: kindLabel(kind),
					variant: kind === 'custom' ? 'outline' : 'secondary'
				});
			}
		},
		{
			accessorKey: 'name',
			header: 'Name',
			cell: ({ row }) => row.original.name
		},
		{
			id: 'holders',
			header: 'Holders',
			cell: ({ row }) => String(row.original.holders?.length ?? 0)
		},
		{
			id: 'description',
			header: 'Description',
			cell: ({ row }) => row.original.description || '—'
		}
	]);

	const table = createSvelteTable({
		get data() {
			return visible;
		},
		get columns() {
			return columns;
		},
		getCoreRowModel: getCoreRowModel(),
		getRowId: (row) => row.name
	});

	async function reload() {
		failed = null;
		try {
			roles = await fetchOrganizationRoles(db);
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Roles could not be loaded.';
			roles = roles ?? [];
		}
	}

	$effect(() => {
		void db;
		void reload();
	});

	function openRole(role: OrgRole) {
		const id = encodeURIComponent(role.name);
		if (detailMode === 'org') {
			void goto(resolve(`/roles/${id}`));
			return;
		}
		void goto(resolve(`/desk/organizations/${encodeURIComponent(db)}/roles/${id}`));
	}

	async function createRole() {
		working = true;
		createError = null;
		try {
			const created = await createOrganizationRole(db, {
				name: slug.trim(),
				label: label.trim(),
				description: description.trim() || undefined
			});
			roles = await fetchOrganizationRoles(db);
			slug = '';
			label = '';
			description = '';
			creating = false;
			openRole(created);
		} catch (error) {
			createError = error instanceof Error ? error.message : 'Role could not be created.';
		} finally {
			working = false;
		}
	}
</script>

<section class="space-y-5">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h2 class="text-lg font-semibold tracking-tight">{title}</h2>
			<p class="text-sm text-muted-foreground">
				App and system roles from installed plugins, plus custom roles for this organization only.
			</p>
		</div>
		<Button size="sm" variant="outline" onclick={() => (creating = !creating)}>
			{creating ? 'Cancel' : 'Create role'}
		</Button>
	</div>

	{#if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{/if}

	{#if creating}
		<div class="space-y-3 rounded-md border border-border bg-card p-4">
			<p class="text-sm text-muted-foreground">
				Custom roles are stored as <span class="font-mono">custom.&lt;slug&gt;</span> and are only
				visible in this organization.
			</p>
			<div class="grid gap-2 sm:grid-cols-2">
				<label class="space-y-1 text-sm">
					<span class="text-muted-foreground">Slug</span>
					<Input
						bind:value={slug}
						placeholder="shift_lead"
						autocomplete="off"
						disabled={working}
					/>
				</label>
				<label class="space-y-1 text-sm">
					<span class="text-muted-foreground">Label</span>
					<Input bind:value={label} placeholder="Shift lead" autocomplete="off" disabled={working} />
				</label>
			</div>
			<label class="block space-y-1 text-sm">
				<span class="text-muted-foreground">Description (optional)</span>
				<Input bind:value={description} autocomplete="off" disabled={working} />
			</label>
			{#if createError}
				<p class="text-sm text-destructive" role="alert">{createError}</p>
			{/if}
			<Button
				size="sm"
				disabled={working || !slug.trim() || !label.trim()}
				onclick={() => void createRole()}
			>
				Create
			</Button>
		</div>
	{/if}

	{#if roles === null}
		<p class="text-sm text-muted-foreground">Loading roles…</p>
	{:else if roles.length === 0}
		<Empty
			icon={ShieldIcon}
			title="No roles"
			description="Install an app that declares roles, or create a custom role above."
		/>
	{:else}
		<div class="flex flex-wrap items-center justify-between gap-3">
			<p class="text-xs text-muted-foreground">
				{visible.length} of {roles.length} roles · click a row to manage holders
			</p>
			<label class="relative block w-full max-w-xs">
				<input
					type="search"
					placeholder="Filter roles"
					aria-label="Filter roles"
					class="h-9 w-full rounded-sm border border-input bg-card px-3 text-sm"
					bind:value={query}
				/>
			</label>
		</div>
		{#if visible.length === 0}
			<p class="text-sm text-muted-foreground">Nothing matches “{query.trim()}”.</p>
		{:else}
			<DataTable {table} onRowClick={openRole} />
		{/if}
	{/if}
</section>

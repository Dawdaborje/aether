<script lang="ts">
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { loadFailure, reportError } from '$lib/errors/report';
	import {
		fetchKernelPermissions,
		setKernelPermissions,
		type KernelPermission
	} from '$lib/org/api';

	let {
		db,
		roleName,
		editable
	}: {
		db: string;
		roleName: string;
		/** A custom role, and the person may manage roles. */
		editable: boolean;
	} = $props();

	let items = $state<KernelPermission[] | null>(null);
	let failed = $state<string | null>(null);
	let saving = $state(false);

	async function load() {
		items = null;
		failed = null;
		try {
			items = await fetchKernelPermissions(db, roleName);
		} catch (error) {
			failed = loadFailure(error, 'Permissions could not be loaded.');
			items = [];
		}
	}

	$effect(() => {
		void db;
		void roleName;
		void load();
	});

	async function toggle(item: KernelPermission, on: boolean) {
		if (!items || saving) return;
		const next = items.map((i) => (i.key === item.key ? { ...i, granted: on } : i));
		saving = true;
		try {
			await setKernelPermissions(
				db,
				roleName,
				next.filter((i) => i.granted && !i.built_in).map((i) => i.key)
			);
			items = next;
		} catch (error) {
			reportError(error, 'Permissions could not be saved.');
			await load();
		} finally {
			saving = false;
		}
	}
</script>

<section class="space-y-3">
	<h2 class="text-base font-medium tracking-tight">Organization permissions</h2>
	{#if items === null}
		<Skeleton class="h-20 w-full rounded-md" />
	{:else if failed}
		<LoadFailed message={failed} onRetry={load} />
	{:else}
		<p class="text-sm text-muted-foreground">
			{#if editable}
				What holders of this role may do to the organization itself. You can only give a permission
				you hold yourself.
			{:else}
				What holders of this role may do to the organization itself.
				{#if items.some((i) => i.built_in)}
					Permissions of a system role cannot be changed.
				{/if}
			{/if}
		</p>
		<ul class="divide-y divide-border rounded-lg border border-border">
			{#each items as item (item.key)}
				<li class="flex items-start gap-3 px-4 py-2.5">
					<Checkbox
						class="mt-0.5"
						checked={item.granted}
						disabled={!editable || item.built_in || saving}
						aria-label={item.label}
						onCheckedChange={(on) => toggle(item, on === true)}
					/>
					<div class="space-y-0.5">
						<p class="text-sm font-medium">{item.label}</p>
						<p class="text-xs text-muted-foreground">{item.description}</p>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</section>

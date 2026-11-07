<script lang="ts">
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { loadFailure, reportError } from '$lib/errors/report';
	import {
		fetchRecordGrants,
		setRecordGrants,
		type RecordGrant,
		type RecordGrantModel
	} from '$lib/org/api';

	let {
		db,
		roleName,
		editable
	}: {
		db: string;
		roleName: string;
		/** A custom role, and the person may change what roles open (roles.manage and org.settings). */
		editable: boolean;
	} = $props();

	let models = $state<RecordGrantModel[] | null>(null);
	let grants = $state<RecordGrant[]>([]);
	let failed = $state<string | null>(null);
	let saving = $state(false);

	const opLabel = { read: 'View', create: 'Create', write: 'Edit', delete: 'Delete' } as const;

	async function load() {
		models = null;
		failed = null;
		try {
			const result = await fetchRecordGrants(db, roleName);
			grants = result.grants;
			models = result.models;
		} catch (error) {
			failed = loadFailure(error, 'Record access could not be loaded.');
			models = [];
		}
	}

	$effect(() => {
		void db;
		void roleName;
		void load();
	});

	function has(model: string, op: string): boolean {
		return grants.find((g) => g.model === model)?.operations.includes(op) ?? false;
	}

	async function toggle(model: string, op: string, on: boolean) {
		if (saving) return;
		const others = grants.filter((g) => g.model !== model);
		const current = grants.find((g) => g.model === model)?.operations ?? [];
		const operations = on ? [...new Set([...current, op])] : current.filter((o) => o !== op);
		const next = operations.length ? [...others, { model, operations }] : others;
		saving = true;
		try {
			await setRecordGrants(db, roleName, next);
			grants = next;
		} catch (error) {
			reportError(error, 'Record access could not be saved.');
			await load();
		} finally {
			saving = false;
		}
	}

	const byPlugin = $derived.by(() => {
		const groups = new Map<string, { label: string; models: RecordGrantModel[] }>();
		for (const item of models ?? []) {
			const group = groups.get(item.plugin) ?? { label: item.plugin_label, models: [] };
			group.models.push(item);
			groups.set(item.plugin, group);
		}
		return [...groups.entries()].map(([key, group]) => ({ key, ...group }));
	});
</script>

<section class="space-y-3">
	<h2 class="text-base font-medium tracking-tight">Record access</h2>
	{#if models === null}
		<Skeleton class="h-24 w-full rounded-md" />
	{:else if failed}
		<LoadFailed message={failed} onRetry={load} />
	{:else if byPlugin.length === 0}
		<p class="text-sm text-muted-foreground">
			No installed plugin restricts who may see or change its records, so there is nothing to open
			for this role.
		</p>
	{:else}
		<p class="text-sm text-muted-foreground">
			{#if editable}
				Open records that a plugin keeps restricted to this role. This only adds access: what a
				plugin's own rules already give, and the limits they put on records (such as “only while
				pending”), still apply.
			{:else}
				Records this role has been opened up to, on top of what the plugins' rules give it.
			{/if}
		</p>
		{#each byPlugin as plugin (plugin.key)}
			<div class="rounded-lg border border-border">
				<h3 class="border-b border-border px-4 py-2 text-sm font-medium">{plugin.label}</h3>
				<ul class="divide-y divide-border">
					{#each plugin.models as item (item.model)}
						<li class="flex flex-wrap items-center gap-x-4 gap-y-1 px-4 py-2.5">
							<span class="min-w-40 text-sm font-medium">{item.model_label}</span>
							{#each item.operations as op (op)}
								<label class="flex items-center gap-1.5 text-sm">
									<Checkbox
										checked={has(item.model, op)}
										disabled={!editable || saving}
										aria-label={`${opLabel[op]} ${item.model_label}`}
										onCheckedChange={(on) => toggle(item.model, op, on === true)}
									/>
									{opLabel[op]}
								</label>
							{/each}
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	{/if}
</section>

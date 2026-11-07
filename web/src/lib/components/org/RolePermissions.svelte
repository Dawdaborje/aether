<script lang="ts">
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { loadFailure } from '$lib/errors/report';
	import { fetchRolePermissions, type RolePermission } from '$lib/org/api';

	let { db, roleName }: { db: string; roleName: string } = $props();

	let permissions = $state<RolePermission[] | null>(null);
	let failed = $state<string | null>(null);

	async function load() {
		permissions = null;
		failed = null;
		try {
			permissions = await fetchRolePermissions(db, roleName);
		} catch (error) {
			failed = loadFailure(error, 'Permissions could not be loaded.');
			permissions = [];
		}
	}

	$effect(() => {
		void db;
		void roleName;
		void load();
	});

	const opLabel = { read: 'View', create: 'Create', write: 'Edit', delete: 'Delete' } as const;
	const opOrder = ['read', 'create', 'write', 'delete'] as const;

	type Model = {
		key: string;
		label: string;
		/** Operations granted on the model, and whether each is limited to some records. */
		ops: Map<RolePermission['operations'][number], boolean>;
		notes: RolePermission[];
	};
	type Plugin = { key: string; label: string; models: Model[] };

	const plugins = $derived.by((): Plugin[] => {
		const byPlugin = new Map<string, { label: string; models: Map<string, Model> }>();
		for (const item of permissions ?? []) {
			const plugin = byPlugin.get(item.plugin) ?? { label: item.plugin_label, models: new Map() };
			const model = plugin.models.get(item.model) ?? {
				key: item.model,
				label: item.model_label,
				ops: new Map(),
				notes: []
			};
			if (item.kind === 'access') {
				for (const op of item.operations) {
					// Unlimited access on any row wins over a limited one.
					model.ops.set(op, (model.ops.get(op) ?? true) && item.conditional);
				}
			} else {
				model.notes.push(item);
			}
			plugin.models.set(item.model, model);
			byPlugin.set(item.plugin, plugin);
		}
		return [...byPlugin.entries()]
			.map(([key, plugin]) => ({
				key,
				label: plugin.label,
				models: [...plugin.models.values()].sort((a, b) => a.label.localeCompare(b.label))
			}))
			.sort((a, b) => a.label.localeCompare(b.label));
	});

	function note(item: RolePermission): string {
		switch (item.kind) {
			case 'field_read':
				return `Can see ${item.name}`;
			case 'field_write':
				return `Can change ${item.name}`;
			case 'transition':
				return `Can move: ${item.name}`;
			default:
				return `Exempt from “${item.name}”`;
		}
	}
</script>

<section class="space-y-3">
	<h2 class="text-base font-medium tracking-tight">Permissions</h2>
	{#if permissions === null}
		<Skeleton class="h-24 w-full rounded-md" />
	{:else if failed}
		<LoadFailed message={failed} onRetry={load} />
	{:else if plugins.length === 0}
		<p class="text-sm text-muted-foreground">
			No installed plugin grants anything to this role through its access rules. Roles can also be
			required by a plugin's commands or pages.
		</p>
	{:else}
		<p class="text-sm text-muted-foreground">
			What holders of this role can do with each kind of record, from the rules of the installed
			plugins. “Some” means only records that match a condition, such as their own.
		</p>
		{#each plugins as plugin (plugin.key)}
			<div class="rounded-lg border border-border">
				<h3 class="border-b border-border px-4 py-2 text-sm font-medium">{plugin.label}</h3>
				<ul class="divide-y divide-border">
					{#each plugin.models as model (model.key)}
						<li class="space-y-1.5 px-4 py-2.5">
							<div class="flex flex-wrap items-center gap-2">
								<span class="min-w-40 text-sm font-medium">{model.label}</span>
								{#each opOrder.filter((op) => model.ops.has(op)) as op (op)}
									<Badge variant="secondary">
										{opLabel[op]}{model.ops.get(op) ? ' (some)' : ''}
									</Badge>
								{/each}
							</div>
							{#each model.notes as item, i (i)}
								<p class="text-xs text-muted-foreground">{note(item)}</p>
							{/each}
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	{/if}
</section>

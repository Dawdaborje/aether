<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect, NativeSelectOption } from '$lib/components/ui/native-select';
	import { Switch } from '$lib/components/ui/switch';
	import type { OrgRole } from '$lib/org/api';
	import { humanizeRoleName, kindLabel, matchesSearch, roleKind } from './roleDisplay';

	let {
		catalog = [],
		assigned = [],
		canManageAdmin = false,
		working = false,
		onGrant,
		onRevoke
	}: {
		/** Every role of the organization. */
		catalog?: OrgRole[];
		/** Role names this member holds. */
		assigned?: string[];
		canManageAdmin?: boolean;
		working?: boolean;
		onGrant: (name: string) => void | Promise<void>;
		onRevoke: (name: string) => void | Promise<void>;
	} = $props();

	let query = $state('');
	/** Which roles to list: every one, those the member holds, or those they do not. */
	let show = $state<'all' | 'assigned' | 'unassigned'>('all');
	/** A workspace group key (see `groupOf`), or `all`. */
	let workspace = $state('all');
	const showOptions = [
		['all', 'All'],
		['assigned', 'Assigned'],
		['unassigned', 'Unassigned']
	] as const;

	type Plugin = { key: string; label: string; roles: OrgRole[]; held: number };
	type Group = { key: string; label: string; plugins: Plugin[]; held: number; total: number };

	/** Held roles the catalog no longer lists (a removed plugin) still show, so they can be revoked. */
	const all = $derived([
		...catalog,
		...assigned
			.filter((name) => !catalog.some((role) => role.name === name))
			.map((name): OrgRole => ({
				name,
				label: humanizeRoleName(name),
				description: 'This role is no longer defined by any installed plugin.',
				is_system: false,
				holders: [],
				plugin: name.includes('.') ? name.split('.')[0] : null
			}))
	]);

	function groupOf(role: OrgRole): { key: string; label: string } {
		const kind = roleKind(role.name, role.is_system);
		if (kind === 'custom') return { key: '~custom', label: 'Custom roles' };
		if (role.workspace)
			return { key: role.workspace, label: role.workspace_label || role.workspace };
		if (kind === 'system') return { key: '~organization', label: 'Organization' };
		return { key: '~other', label: 'Other' };
	}

	function pluginOf(role: OrgRole): { key: string; label: string } {
		if (role.plugin) return { key: role.plugin, label: role.plugin_label || role.plugin };
		const kind = roleKind(role.name, role.is_system);
		return kind === 'custom'
			? { key: '~custom', label: 'Defined in this organization' }
			: { key: '~core', label: 'Access and administration' };
	}

	/** The workspaces roles belong to, for the filter. */
	const workspaceOptions = $derived.by(() => {
		const found = new Map<string, string>();
		for (const role of all) {
			const g = groupOf(role);
			found.set(g.key, g.label);
		}
		return [...found.entries()]
			.map(([key, label]) => ({ key, label }))
			.sort((a, b) => {
				const rank = (key: string) => (key === '~organization' ? 0 : key.startsWith('~') ? 2 : 1);
				return rank(a.key) - rank(b.key) || a.label.localeCompare(b.label);
			});
	});

	const groups = $derived.by((): Group[] => {
		const needle = query.trim();
		const byGroup = new Map<string, { label: string; plugins: Map<string, Plugin> }>();
		for (const role of all) {
			const isHeld = assigned.includes(role.name);
			if (show === 'assigned' && !isHeld) continue;
			if (show === 'unassigned' && isHeld) continue;
			const g = groupOf(role);
			if (workspace !== 'all' && g.key !== workspace) continue;
			const p = pluginOf(role);
			if (
				needle &&
				!matchesSearch(
					[
						role.name,
						role.label,
						role.description,
						p.label,
						g.label,
						kindLabel(roleKind(role.name, role.is_system))
					],
					needle
				)
			)
				continue;
			const group = byGroup.get(g.key) ?? { label: g.label, plugins: new Map() };
			const plugin = group.plugins.get(p.key) ?? { key: p.key, label: p.label, roles: [], held: 0 };
			plugin.roles.push(role);
			if (isHeld) plugin.held += 1;
			group.plugins.set(p.key, plugin);
			byGroup.set(g.key, group);
		}
		return [...byGroup.entries()]
			.map(([key, group]) => {
				const plugins = [...group.plugins.values()].sort((a, b) => a.label.localeCompare(b.label));
				for (const plugin of plugins) plugin.roles.sort((a, b) => a.label.localeCompare(b.label));
				return {
					key,
					label: group.label,
					plugins,
					held: plugins.reduce((sum, plugin) => sum + plugin.held, 0),
					total: plugins.reduce((sum, plugin) => sum + plugin.roles.length, 0)
				};
			})
			.sort((a, b) => {
				// Organization first, custom last, the rest by name.
				const rank = (key: string) => (key === '~organization' ? 0 : key.startsWith('~') ? 2 : 1);
				return rank(a.key) - rank(b.key) || a.label.localeCompare(b.label);
			});
	});

	const shown = $derived(groups.reduce((sum, group) => sum + group.total, 0));
	const filtering = $derived(query.trim() !== '' || show !== 'all' || workspace !== 'all');

	function locked(role: OrgRole): boolean {
		return role.name === 'org_admin' && !canManageAdmin;
	}
</script>

<div class="space-y-5">
	<div class="flex flex-wrap items-center gap-3">
		<Input
			type="search"
			placeholder="Search roles"
			aria-label="Search roles"
			class="max-w-xs"
			bind:value={query}
		/>
		{#if workspaceOptions.length > 1}
			<NativeSelect
				class="w-52"
				aria-label="Workspace"
				value={workspace}
				onchange={(event) => (workspace = event.currentTarget.value)}
			>
				<NativeSelectOption value="all">All workspaces</NativeSelectOption>
				{#each workspaceOptions as option (option.key)}
					<NativeSelectOption value={option.key}>{option.label}</NativeSelectOption>
				{/each}
			</NativeSelect>
		{/if}
		<div class="flex items-center gap-1.5" role="group" aria-label="Show roles">
			{#each showOptions as [id, label] (id)}
				<Button
					size="xs"
					variant={show === id ? 'default' : 'outline'}
					aria-pressed={show === id}
					onclick={() => (show = id)}
				>
					{label}
				</Button>
			{/each}
		</div>
		<p class="ml-auto text-sm text-muted-foreground" aria-live="polite">
			{assigned.length} assigned{filtering ? ` · ${shown} shown` : ` of ${all.length}`}
		</p>
	</div>

	{#each groups as group (group.key)}
		<section class="space-y-2">
			<h3
				class="flex items-baseline gap-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase"
			>
				{group.label}
				<span class="font-normal normal-case">{group.held} of {group.total}</span>
			</h3>
			<div class="space-y-2">
				{#each group.plugins as plugin (plugin.key)}
					<details
						class="group rounded-lg border border-border"
						open={plugin.held > 0 ||
							query.trim() !== '' ||
							workspace !== 'all' ||
							show === 'unassigned' ||
							group.plugins.length === 1}
					>
						<summary
							class="flex cursor-pointer list-none items-center gap-3 px-4 py-2.5 text-sm select-none"
						>
							<span
								class="text-muted-foreground transition-transform group-open:rotate-90"
								aria-hidden="true">›</span
							>
							<span class="font-medium">{plugin.label}</span>
							<span class="ml-auto flex items-center gap-2">
								{#if plugin.held > 0}
									<Badge variant="secondary">{plugin.held} of {plugin.roles.length}</Badge>
								{:else}
									<span class="text-xs text-muted-foreground">{plugin.roles.length} available</span>
								{/if}
							</span>
						</summary>
						<ul class="divide-y divide-border border-t border-border">
							{#each plugin.roles as role (role.name)}
								{@const held = assigned.includes(role.name)}
								<li class="flex items-start gap-3 px-4 py-2.5">
									<div class="min-w-0 flex-1">
										<p class="text-sm font-medium" class:text-muted-foreground={!held}>
											{role.label}
										</p>
										{#if role.description}
											<p class="text-xs text-muted-foreground">{role.description}</p>
										{/if}
										<p class="font-mono text-xs text-muted-foreground/70">{role.name}</p>
									</div>
									<Switch
										size="sm"
										aria-label={`${held ? 'Revoke' : 'Grant'} ${role.label}`}
										title={locked(role) ? 'Only an admin can change this role' : undefined}
										bind:checked={
											() => held,
											(checked) => void (checked ? onGrant(role.name) : onRevoke(role.name))
										}
										disabled={working || locked(role)}
									/>
								</li>
							{/each}
						</ul>
					</details>
				{/each}
			</div>
		</section>
	{:else}
		<p class="text-sm text-muted-foreground">
			{filtering ? 'No role matches.' : 'This organization has no roles yet.'}
		</p>
	{/each}
</div>

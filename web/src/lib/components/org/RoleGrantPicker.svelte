<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Field from '$lib/components/ui/field/index.js';
	import { Input } from '$lib/components/ui/input';
	import { Select } from '$lib/components/ui/select';
	import type { OrgRole } from '$lib/org/api';
	import { kindLabel, matchesSearch, roleKind } from './roleDisplay';

	let {
		roles = [],
		disabled = false,
		working = false,
		onGrant
	}: {
		/** Roles not yet held (already filtered by caller). */
		roles?: OrgRole[];
		disabled?: boolean;
		working?: boolean;
		onGrant: (names: string[]) => void | Promise<void>;
	} = $props();

	let query = $state('');
	let workspace = $state('');
	let plugin = $state('');
	let selected = new SvelteSet<string>();

	/** The areas the grantable roles come from, with how many roles each holds. */
	const workspaceOptions = $derived.by(() => {
		const counts = new Map<string, { label: string; count: number }>();
		for (const role of roles) {
			if (!role.workspace) continue;
			const entry = counts.get(role.workspace) ?? {
				label: role.workspace_label || role.workspace,
				count: 0
			};
			entry.count += 1;
			counts.set(role.workspace, entry);
		}
		return [...counts.entries()]
			.sort((a, b) => a[1].label.localeCompare(b[1].label))
			.map(([value, { label, count }]) => ({ value, label: `${label} (${count})` }));
	});

	/** Plugins, narrowed to the chosen workspace. */
	const pluginOptions = $derived.by(() => {
		const counts = new Map<string, { label: string; count: number }>();
		for (const role of roles) {
			if (!role.plugin) continue;
			if (workspace && role.workspace !== workspace) continue;
			const entry = counts.get(role.plugin) ?? { label: role.plugin_label || role.plugin, count: 0 };
			entry.count += 1;
			counts.set(role.plugin, entry);
		}
		return [...counts.entries()]
			.sort((a, b) => a[1].label.localeCompare(b[1].label))
			.map(([value, { label, count }]) => ({ value, label: `${label} (${count})` }));
	});

	// A plugin left over from another workspace no longer applies.
	$effect(() => {
		if (plugin && !pluginOptions.some((option) => option.value === plugin)) plugin = '';
	});

	const filtered = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		return roles.filter((role) => {
			if (workspace && role.workspace !== workspace) return false;
			if (plugin && role.plugin !== plugin) return false;
			if (!needle) return true;
			return matchesSearch(
				[
					role.name,
					role.label,
					role.description,
					role.plugin,
					role.plugin_label,
					role.workspace_label,
					kindLabel(roleKind(role.name, role.is_system))
				],
				needle
			);
		});
	});

	const narrowed = $derived(workspace !== '' || plugin !== '' || query.trim() !== '');

	function toggle(name: string, checked: boolean) {
		if (checked) selected.add(name);
		else selected.delete(name);
	}

	const allFilteredSelected = $derived(
		filtered.length > 0 && filtered.every((role) => selected.has(role.name))
	);

	/** Select every role that matches the current filters (roles picked earlier stay picked). */
	function selectAllFiltered() {
		for (const role of filtered) selected.add(role.name);
	}

	async function grant() {
		const names = [...selected];
		if (names.length === 0) return;
		await onGrant(names);
		selected.clear();
		query = '';
		workspace = '';
		plugin = '';
	}
</script>

<Field.Set>
	<Field.Legend variant="label">Grant roles</Field.Legend>
	<Field.Description>Search, select one or more, then grant them together.</Field.Description>
	{#if roles.length === 0}
		<p class="pt-2 text-sm text-muted-foreground">Every grantable role is already assigned.</p>
	{:else}
		<div class="space-y-3 pt-2">
			<Input
				type="search"
				placeholder="Search roles"
				aria-label="Search roles"
				bind:value={query}
				{disabled}
			/>
			{#if workspaceOptions.length > 0 || pluginOptions.length > 0}
				<div class="grid gap-2 sm:grid-cols-2">
					{#if workspaceOptions.length > 1}
						<Select
							placeholder="All areas"
							clearLabel="All areas"
							options={workspaceOptions}
							bind:value={workspace}
							{disabled}
						/>
					{/if}
					{#if pluginOptions.length > 1}
						<Select
							placeholder="All apps"
							clearLabel="All apps"
							options={pluginOptions}
							bind:value={plugin}
							{disabled}
						/>
					{/if}
				</div>
			{/if}
			<p class="text-xs text-muted-foreground" aria-live="polite">
				{filtered.length} of {roles.length} role{roles.length === 1 ? '' : 's'}
				{#if narrowed}
					·
					<button
						type="button"
						class="underline underline-offset-2 hover:text-foreground"
						onclick={() => {
							query = '';
							workspace = '';
							plugin = '';
						}}>clear filters</button
					>
				{/if}
			</p>
			{#if filtered.length > 0}
				<div class="flex flex-wrap items-center gap-2">
					<Button
						size="sm"
						variant="outline"
						disabled={disabled || working || allFilteredSelected}
						onclick={selectAllFiltered}
					>
						Select all {filtered.length}{narrowed ? ' matching' : ''}
					</Button>
					{#if selected.size > 0}
						<Button
							size="sm"
							variant="ghost"
							disabled={disabled || working}
							onclick={() => selected.clear()}
						>
							Clear selection ({selected.size})
						</Button>
					{/if}
				</div>
			{/if}
			{#if filtered.length === 0}
				<p class="text-sm text-muted-foreground">No role matches these filters.</p>
			{:else}
				<Field.Group class="max-h-[32rem] gap-2 overflow-y-auto pr-1">
					{#each filtered as role (role.name)}
						{@const kind = roleKind(role.name, role.is_system)}
						<Field.Field orientation="horizontal" class="items-start rounded-sm border border-border/60 px-2 py-2">
							<Checkbox
								id={`grant-role-${role.name}`}
								checked={selected.has(role.name)}
								onCheckedChange={(checked) => toggle(role.name, checked === true)}
								disabled={disabled || working}
							/>
							<Field.Label for={`grant-role-${role.name}`} class="min-w-0 flex-1 font-normal">
								<span class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
									<span class="font-medium">{role.label}</span>
									<span class="font-mono text-xs text-muted-foreground">{role.name}</span>
									<span class="text-xs uppercase tracking-wide text-muted-foreground"
										>{kindLabel(kind)}</span
									>
								</span>
								{#if role.description}
									<span class="mt-0.5 block text-xs text-muted-foreground">{role.description}</span>
								{/if}
							</Field.Label>
						</Field.Field>
					{/each}
				</Field.Group>
			{/if}
			<Button size="sm" disabled={disabled || working || selected.size === 0} onclick={() => void grant()}>
				Grant {selected.size > 0 ? `(${selected.size})` : 'selected'}
			</Button>
		</div>
	{/if}
</Field.Set>

<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { Button } from '$lib/components/ui/button';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Field from '$lib/components/ui/field/index.js';
	import { Input } from '$lib/components/ui/input';
	import type { OrgRole } from '$lib/org/api';
	import { kindLabel, roleKind } from './roleDisplay';

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
	let selected = new SvelteSet<string>();

	const filtered = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return roles;
		return roles.filter((role) => {
			const hay = [role.name, role.label, role.description ?? '', kindLabel(roleKind(role.name, role.is_system))]
				.join(' ')
				.toLowerCase();
			return hay.includes(needle);
		});
	});

	function toggle(name: string, checked: boolean) {
		if (checked) selected.add(name);
		else selected.delete(name);
	}

	async function grant() {
		const names = [...selected];
		if (names.length === 0) return;
		await onGrant(names);
		selected.clear();
		query = '';
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
				placeholder="Filter roles"
				aria-label="Filter roles"
				bind:value={query}
				{disabled}
			/>
			{#if filtered.length === 0}
				<p class="text-sm text-muted-foreground">Nothing matches “{query.trim()}”.</p>
			{:else}
				<Field.Group class="max-h-64 gap-2 overflow-y-auto pr-1">
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

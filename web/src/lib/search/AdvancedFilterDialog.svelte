<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import type { PageChrome } from '$lib/pages/pageChrome.svelte';
	import {
		OP_LABELS,
		describeGroup,
		emptyGroup,
		groupToFilter,
		isGroup,
		newRule,
		operatorsFor,
		takesNoValue,
		valueKind
	} from './advanced';
	import { makeChip } from './filterCodec';
	import type { AdvancedValue, FieldDef, Rule, RuleGroup } from './types';

	let { chrome, open = $bindable(false) }: { chrome: PageChrome; open?: boolean } = $props();

	const fields = $derived(chrome.fieldDefs);
	const existing = $derived(chrome.chips.find((chip) => chip.op === 'domain'));

	let root = $state<RuleGroup>(emptyGroup());

	// Opening the dialog starts from the custom filter already applied, or a single empty rule.
	$effect(() => {
		if (!open) return;
		const value = existing?.value as AdvancedValue | undefined;
		const tree = value?.tree ? structuredClone($state.snapshot(value.tree)) : emptyGroup();
		if (tree.items.length === 0) tree.items.push(newRule(fields));
		root = tree;
	});

	const fieldOf = (name: string): FieldDef | undefined => fields.find((f) => f.field === name);

	function changeField(rule: Rule, name: string) {
		rule.field = name;
		const ops = operatorsFor(valueKind(fieldOf(name)));
		if (!ops.includes(rule.op)) rule.op = ops[0]!;
		rule.value = '';
		rule.value2 = '';
	}

	function remove(group: RuleGroup, index: number) {
		group.items.splice(index, 1);
	}

	function addRule(group: RuleGroup) {
		group.items.push(newRule(fields));
	}

	function addGroup(group: RuleGroup) {
		group.items.push({ mode: 'or', items: [newRule(fields)] });
	}

	const preview = $derived(describeGroup(root, fields));

	function apply() {
		const tree = $state.snapshot(root) as RuleGroup;
		const filter = groupToFilter(tree, fields);
		const others = chrome.chips.filter((chip) => chip.op !== 'domain');
		if (Object.keys(filter).length === 0) {
			chrome.setChips(others);
		} else {
			const value: AdvancedValue = { filter, tree };
			chrome.setChips([
				...others,
				makeChip({
					field: '_adv',
					op: 'domain',
					value,
					label: describeGroup(tree, fields) || 'Custom filter'
				})
			]);
		}
		open = false;
	}

	function clearAll() {
		chrome.setChips(chrome.chips.filter((chip) => chip.op !== 'domain'));
		open = false;
	}

	const control =
		'h-9 rounded-field border-0 border-b border-border-strong bg-field px-2 text-sm text-foreground hover:bg-field-hover';
</script>

{#snippet groupView(group: RuleGroup, depth: number)}
	<div class="space-y-2 {depth > 0 ? 'rounded-md border border-border bg-muted/30 p-3' : ''}">
		{#if group.items.length > 1 || depth > 0}
			<p class="flex flex-wrap items-center gap-2 text-sm">
				Match
				<select class={control} bind:value={group.mode} aria-label="How the rules combine">
					<option value="and">all</option>
					<option value="or">any</option>
				</select>
				of the following rules
			</p>
		{/if}
		{#each group.items as item, index (index)}
			<div class="flex flex-wrap items-center gap-2">
				{#if isGroup(item)}
					<div class="min-w-0 flex-1">{@render groupView(item, depth + 1)}</div>
				{:else}
					{@const def = fieldOf(item.field)}
					{@const kind = valueKind(def)}
					<select
						class="{control} max-w-48"
						value={item.field}
						aria-label="Field"
						onchange={(e) => changeField(item, e.currentTarget.value)}
					>
						{#each fields as f (f.field)}
							<option value={f.field}>{f.label}</option>
						{/each}
					</select>
					<select class={control} bind:value={item.op} aria-label="Operator">
						{#each operatorsFor(kind) as op (op)}
							<option value={op}>{OP_LABELS[op]}</option>
						{/each}
					</select>
					{#if !takesNoValue(item.op)}
						{#if kind === 'boolean'}
							<select class={control} bind:value={item.value} aria-label="Value">
								<option value="true">Yes</option>
								<option value="false">No</option>
							</select>
						{:else if kind === 'choice' && (item.op === 'eq' || item.op === 'ne')}
							<select class={control} bind:value={item.value} aria-label="Value">
								<option value="">Choose…</option>
								{#each def?.options ?? [] as option (String(option.value))}
									<option value={String(option.value)}>{option.label}</option>
								{/each}
							</select>
						{:else}
							{@const inputType =
								kind === 'number'
									? 'number'
									: kind === 'date'
										? 'date'
										: kind === 'datetime'
											? 'datetime-local'
											: 'text'}
							<input
								class="{control} w-40"
								type={inputType}
								step={kind === 'number' ? 'any' : undefined}
								bind:value={item.value}
								aria-label="Value"
								placeholder={item.op === 'in' || item.op === 'nin' ? 'a, b, c' : 'Value'}
							/>
							{#if item.op === 'between'}
								and
								<input
									class="{control} w-40"
									type={inputType}
									step={kind === 'number' ? 'any' : undefined}
									bind:value={item.value2}
									aria-label="Upper value"
								/>
							{/if}
						{/if}
					{/if}
				{/if}
				<Button
					variant="ghost"
					size="icon-sm"
					aria-label="Remove rule"
					onclick={() => remove(group, index)}
				>
					<AppIcon name="close" class="size-4" />
				</Button>
			</div>
		{/each}
		<div class="flex gap-2">
			<Button variant="ghost" size="sm" onclick={() => addRule(group)}>
				<AppIcon name="plus" data-icon="inline-start" /> Add rule
			</Button>
			<Button variant="ghost" size="sm" onclick={() => addGroup(group)}>
				<AppIcon name="plus" data-icon="inline-start" /> Add group
			</Button>
		</div>
	</div>
{/snippet}

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-3xl">
		<Dialog.Header>
			<Dialog.Title>Custom filter</Dialog.Title>
			<Dialog.Description>
				Combine rules on any column: ages from a number up, names starting with a letter, dates
				between two days. Use a group to mix “all” and “any”.
			</Dialog.Description>
		</Dialog.Header>
		{#if fields.length === 0}
			<p class="text-sm text-muted-foreground">This list has no columns to filter on.</p>
		{:else}
			<div class="max-h-[60vh] overflow-y-auto pr-1">{@render groupView(root, 0)}</div>
			{#if preview}
				<p class="text-xs text-muted-foreground">Shows records where {preview}.</p>
			{/if}
		{/if}
		<Dialog.Footer>
			{#if existing}
				<Button variant="ghost" onclick={clearAll}>Clear custom filter</Button>
			{/if}
			<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
			<Button onclick={apply} disabled={fields.length === 0}>Apply</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

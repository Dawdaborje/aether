<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Popover from '$lib/components/ui/popover';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import type { PageChrome } from '$lib/pages/pageChrome.svelte';
	import { makeChip } from './filterCodec';
	import AdvancedFilterDialog from './AdvancedFilterDialog.svelte';

	let { chrome }: { chrome: PageChrome } = $props();

	let open = $state(false);
	let advancedOpen = $state(false);
	const hasCustom = $derived(chrome.chips.some((chip) => chip.op === 'domain'));

	/** The values ticked for `field`: one from an `eq` chip, several from an `in` chip. */
	function selectedValues(field: string): string[] {
		const chip = chrome.chips.find(
			(row) => row.field === field && (row.op === 'eq' || row.op === 'in')
		);
		if (chip == null || chip.value == null || chip.value === '') return [];
		return Array.isArray(chip.value) ? chip.value.map(String) : [String(chip.value)];
	}

	/** Tick or untick one option. One ticked value is `is X`; several are `is X, Y` (any of them). */
	function toggleOption(field: string, raw: string) {
		const preset = chrome.filterPresets.find((row) => row.field === field);
		const current = selectedValues(field);
		const next = current.includes(raw) ? current.filter((v) => v !== raw) : [...current, raw];
		const without = chrome.chips.filter((chip) => chip.field !== field);
		if (next.length === 0) {
			chrome.setChips(without);
			return;
		}
		const labels = next.map(
			(value) => preset?.options.find((o) => String(o.value) === value)?.label ?? value
		);
		const name = preset?.label ?? field;
		const label = `${name} is ${labels.join(' or ')}`;
		if (next.length === 1) {
			const option = preset?.options.find((o) => String(o.value) === next[0]);
			chrome.setChips([
				...without,
				makeChip({ field, op: 'eq', value: option?.value ?? next[0], label })
			]);
		} else {
			chrome.setChips([...without, makeChip({ field, op: 'in', value: next, label })]);
		}
	}
</script>

{#if chrome.filterPresets.length > 0 || chrome.fieldDefs.length > 0}
	<Popover.Root bind:open>
		<Popover.Trigger>
			{#snippet child({ props })}
				<Button variant="outline" {...props}>
					<AppIcon name="filter" data-icon="inline-start" />
					Filters
				</Button>
			{/snippet}
		</Popover.Trigger>
		<Popover.Content class="w-72 space-y-3 p-3" align="end">
			<p class="text-xs font-medium text-muted-foreground">Filters</p>
			{#each chrome.filterPresets as preset (preset.field)}
				{@const picked = selectedValues(preset.field)}
				<fieldset class="space-y-1">
					<legend class="text-xs text-muted-foreground">{preset.label}</legend>
					<div class="max-h-48 space-y-0.5 overflow-y-auto">
						{#each preset.options as option (`${preset.field}-${String(option.value)}`)}
							<label
								class="flex cursor-pointer items-center gap-2 rounded-field px-1 py-1.5 text-sm hover:bg-muted"
							>
								<Checkbox
									checked={picked.includes(String(option.value))}
									onCheckedChange={() => toggleOption(preset.field, String(option.value))}
								/>
								{option.label}
							</label>
						{/each}
					</div>
				</fieldset>
			{/each}
			{#if chrome.fieldDefs.length > 0}
				<Button
					variant="outline"
					class="w-full"
					onclick={() => {
						open = false;
						advancedOpen = true;
					}}
				>
					{hasCustom ? 'Edit custom filter…' : 'Add custom filter…'}
				</Button>
			{/if}
		</Popover.Content>
	</Popover.Root>
	<AdvancedFilterDialog {chrome} bind:open={advancedOpen} />
{/if}

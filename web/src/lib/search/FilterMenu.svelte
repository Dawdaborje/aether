<script lang="ts">
	import FilterIcon from '@lucide/svelte/icons/list-filter';
	import { Button } from '$lib/components/ui/button';
	import * as Popover from '$lib/components/ui/popover';
	import type { PageChrome } from '$lib/pages/pageChrome.svelte';
	import { makeChip } from './filterCodec';

	let { chrome }: { chrome: PageChrome } = $props();

	let open = $state(false);

	function applyPreset(field: string, raw: string) {
		const preset = chrome.filterPresets.find((row) => row.field === field);
		const without = chrome.chips.filter((chip) => chip.field !== field);
		if (!raw) {
			chrome.setChips(without);
			return;
		}
		const option = preset?.options.find((row) => String(row.value) === raw);
		const value = option?.value ?? raw;
		const valueLabel = option?.label ?? raw;
		chrome.setChips([
			...without,
			makeChip({
				field,
				op: 'eq',
				value,
				label: `${preset?.label ?? field} is ${valueLabel}`
			})
		]);
	}

	function currentValue(field: string): string {
		const chip = chrome.chips.find((row) => row.field === field && row.op === 'eq');
		return chip == null ? '' : String(chip.value ?? '');
	}
</script>

{#if chrome.filterPresets.length > 0}
	<Popover.Root bind:open>
		<Popover.Trigger>
			{#snippet child({ props })}
				<Button variant="outline" size="sm" {...props}>
					<FilterIcon data-icon="inline-start" />
					Filters
				</Button>
			{/snippet}
		</Popover.Trigger>
		<Popover.Content class="w-72 space-y-3 p-3" align="end">
			<p class="text-xs font-medium text-muted-foreground">Filters</p>
			{#each chrome.filterPresets as preset (preset.field)}
				<label class="block space-y-1">
					<span class="text-xs text-muted-foreground">{preset.label}</span>
					<select
						class="border-input bg-background flex h-9 w-full rounded-md border px-2 text-sm"
						value={currentValue(preset.field)}
						onchange={(event) => applyPreset(preset.field, event.currentTarget.value)}
					>
						<option value="">All</option>
						{#each preset.options as option (`${preset.field}-${String(option.value)}`)}
							<option value={String(option.value)}>{option.label}</option>
						{/each}
					</select>
				</label>
			{/each}
		</Popover.Content>
	</Popover.Root>
{/if}

<script lang="ts">
	import LayersIcon from '@lucide/svelte/icons/layers';
	import { Button } from '$lib/components/ui/button';
	import * as Popover from '$lib/components/ui/popover';
	import type { PageChrome } from '$lib/pages/pageChrome.svelte';

	let { chrome }: { chrome: PageChrome } = $props();

	let open = $state(false);

	function toggle(field: string) {
		if (chrome.groupBy.includes(field)) {
			chrome.setGroupBy(chrome.groupBy.filter((name) => name !== field));
		} else {
			chrome.setGroupBy([...chrome.groupBy, field]);
		}
	}
</script>

{#if chrome.groupByOptions.length > 0}
	<Popover.Root bind:open>
		<Popover.Trigger>
			{#snippet child({ props })}
				<Button variant="outline" size="sm" {...props}>
					<LayersIcon data-icon="inline-start" />
					Group By
					{#if chrome.groupBy.length > 0}
						<span class="text-muted-foreground">({chrome.groupBy.length})</span>
					{/if}
				</Button>
			{/snippet}
		</Popover.Trigger>
		<Popover.Content class="w-56 space-y-1 p-2" align="end">
			<p class="px-1 pb-1 text-xs font-medium text-muted-foreground">Group By</p>
			{#each chrome.groupByOptions as option (option.field)}
				<label
					class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted"
				>
					<input
						type="checkbox"
						class="size-3.5"
						checked={chrome.groupBy.includes(option.field)}
						onchange={() => toggle(option.field)}
					/>
					{option.label}
				</label>
			{/each}
			{#if chrome.groupBy.length > 0}
				<Button
					variant="ghost"
					size="sm"
					class="mt-1 w-full"
					onclick={() => chrome.setGroupBy([])}
				>
					Clear
				</Button>
			{/if}
		</Popover.Content>
	</Popover.Root>
{/if}

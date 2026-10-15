<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { companyStore } from '$lib/company/companyStore.svelte';
</script>

<!-- The working company, when the organization has the company plugin. -->
{#if companyStore.available && companyStore.currentName}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			disabled={!companyStore.canSwitch || companyStore.switching}
			aria-label="Company"
			title="Working company"
			class="flex h-8 max-w-44 items-center gap-1.5 rounded-md px-2.5 text-sm text-region-muted-foreground transition outline-none enabled:cursor-pointer enabled:hover:bg-region-hover enabled:hover:text-region-hover-foreground disabled:cursor-default data-[state=open]:bg-region-hover"
		>
			<span class="truncate">{companyStore.currentName}</span>
			{#if companyStore.canSwitch}<ChevronDownIcon class="size-3 shrink-0 opacity-60" />{/if}
		</DropdownMenu.Trigger>
		{#if companyStore.canSwitch}
			<DropdownMenu.Content align="end" class="min-w-52">
				{#each companyStore.companies as company (company.id)}
					<DropdownMenu.Item
						disabled={companyStore.switching || company.id === companyStore.current?.id}
						onclick={() => void companyStore.choose(company.id)}
					>
						<span class="flex-1 truncate">{company.name}</span>
						{#if company.is_default}
							<span class="text-xs text-muted-foreground">Default</span>
						{/if}
					</DropdownMenu.Item>
				{/each}
			</DropdownMenu.Content>
		{/if}
	</DropdownMenu.Root>
{/if}

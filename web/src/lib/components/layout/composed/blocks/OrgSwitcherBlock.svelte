<script lang="ts">
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { orgStore } from '$lib/org/orgStore.svelte';

	async function leaveOrganization() {
		await orgStore.leave();
		if (!orgStore.error) await goto(resolve('/desk/organizations'));
	}
</script>

<!-- The current organization; switch or leave from here. -->
{#if orgStore.currentName}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			disabled={!orgStore.canSwitch && !orgStore.canLeave}
			aria-label="Organization"
			class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-region-muted-foreground transition outline-none enabled:cursor-pointer enabled:hover:bg-region-hover enabled:hover:text-region-hover-foreground disabled:cursor-default data-[state=open]:bg-region-hover"
		>
			<span class="size-1.5 rounded-full bg-region-primary" aria-hidden="true"></span>
			<span class="max-w-40 truncate">{orgStore.currentName}</span>
			{#if orgStore.canSwitch || orgStore.canLeave}<ChevronDownIcon
					class="size-3 opacity-60"
				/>{/if}
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="end" class="min-w-52">
			{#if orgStore.canSwitch}
				<DropdownMenu.Item onclick={() => orgStore.openSwitcher()}
					>Switch organization</DropdownMenu.Item
				>
			{/if}
			{#if orgStore.canLeave}
				<DropdownMenu.Item onclick={leaveOrganization}>Leave organization</DropdownMenu.Item>
			{/if}
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/if}

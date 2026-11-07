<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { callPlugin } from '$lib/pages/pageContext.svelte';
	import type { LayoutBlock } from '$lib/theme/types';
	import { textProp } from '../visibility';
	import { openItem } from '../nav/openItem';
	import { parseScopes, type Scopes } from '../scopes';

	let { block }: { block: LayoutBlock; vertical?: boolean } = $props();
	const source = $derived(textProp(block, 'source'));
	const title = $derived(textProp(block, 'label', 'Scope'));

	let scopes = $state<Scopes>({ current: null, items: [] });
	let token = 0;

	async function load() {
		const mine = ++token;
		const dot = source.indexOf('.');
		if (dot <= 0) return;
		const result = await callPlugin(source.slice(0, dot), source.slice(dot + 1));
		if (mine !== token) return;
		scopes = result.ok ? parseScopes(result.data) : { current: null, items: [] };
	}

	$effect(() => {
		void source;
		void load();
	});
	// The current scope can change with the page (a project's page selects that project).
	afterNavigate(() => void load());
</script>

<!-- A plugin-defined scope (project, group, team): the current one, and the others to switch to. -->
{#if scopes.items.length > 0}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			data-part="shell.scope"
			aria-label={title}
			class="flex h-8 items-center gap-1.5 rounded-button px-2.5 text-sm text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring data-[state=open]:bg-region-hover"
		>
			<span class="max-w-44 truncate">{scopes.current?.label ?? title}</span>
			<AppIcon name="chevrons-up-down" class="size-3.5 opacity-60" />
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="start" class="min-w-52">
			<DropdownMenu.Label>{title}</DropdownMenu.Label>
			{#each scopes.items as item (item.id)}
				<DropdownMenu.Item
					onclick={() => openItem({ label: item.label, href: item.href })}
					aria-current={item.id === scopes.current?.id ? 'true' : undefined}
					class="aria-[current=true]:font-medium"
				>
					<span class="truncate">{item.label}</span>
					{#if item.id === scopes.current?.id}
						<AppIcon name="check" class="ml-auto size-3.5" />
					{/if}
				</DropdownMenu.Item>
			{/each}
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/if}

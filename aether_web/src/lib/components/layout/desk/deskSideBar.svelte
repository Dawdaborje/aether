<script lang="ts">
	import { base } from '$app/paths';
	import FileTextIcon from '@lucide/svelte/icons/file-text';
	import LayoutDashboardIcon from '@lucide/svelte/icons/layout-dashboard';
	import SettingsIcon from '@lucide/svelte/icons/settings';
	import Building2Icon from '@lucide/svelte/icons/building-2';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';

	const nav = [
		{
			label: 'Workspace',
			items: [
				{ label: 'Apps', href: `${base}/apps`, icon: LayoutDashboardIcon },
				{ label: 'Invoices', href: `${base}/accounting/invoices`, icon: FileTextIcon },
				{ label: 'Organizations', href: `${base}/organizations`, icon: Building2Icon }
			]
		},
		{
			label: 'System',
			items: [{ label: 'Settings', href: `${base}/settings`, icon: SettingsIcon }]
		}
	];
</script>

<Sidebar.Root collapsible="icon" class="border-r border-sidebar-border">
	<Sidebar.Header class="px-3 py-4">
		<div class="flex items-center gap-2 px-1">
			<div
				class="flex size-7 items-center justify-center rounded bg-sidebar-primary text-xs font-bold text-sidebar-primary-foreground"
			>
				A
			</div>
			<span
				class="text-sm font-semibold tracking-tight text-sidebar-foreground group-data-[collapsible=icon]:hidden"
			>
				Aether
			</span>
		</div>
	</Sidebar.Header>
	<Sidebar.Content>
		{#each nav as group (group.label)}
			<Sidebar.Group>
				<Sidebar.GroupLabel>{group.label}</Sidebar.GroupLabel>
				<Sidebar.GroupContent>
					<Sidebar.Menu>
						{#each group.items as item (item.href)}
							{@const Icon = item.icon}
							<Sidebar.MenuItem>
								<Sidebar.MenuButton>
									{#snippet child({ props })}
										<a href={item.href} {...props}>
											<Icon class="size-4" />
											<span>{item.label}</span>
										</a>
									{/snippet}
								</Sidebar.MenuButton>
							</Sidebar.MenuItem>
						{/each}
					</Sidebar.Menu>
				</Sidebar.GroupContent>
			</Sidebar.Group>
		{/each}
	</Sidebar.Content>
</Sidebar.Root>

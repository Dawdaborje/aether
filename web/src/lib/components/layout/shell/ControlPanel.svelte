<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import * as Breadcrumb from '$lib/components/ui/breadcrumb/index.js';

	type Crumb = { label: string; href?: string };

	let {
		crumbs,
		actions,
		children
	}: {
		/** Trail from the app down to this page; the last entry is the current page. */
		crumbs: Crumb[];
		/** Primary buttons, shown on the left under the trail (Odoo's "New" position). */
		actions?: Snippet;
		/** Right-hand controls: search, filters, view switcher. */
		children?: Snippet;
	} = $props();
</script>

<!-- Odoo's control panel: breadcrumbs, the page's main actions, and its search and view controls. -->
<div class="border-b border-border bg-card/80 backdrop-blur">
	<div class="flex w-full flex-wrap items-center gap-x-6 gap-y-1 px-4 py-2 md:px-6">
		<div class="min-w-0 space-y-1.5">
			<Breadcrumb.Root>
				<Breadcrumb.List class="text-[15px] font-semibold normal-case tracking-tight">
					{#each crumbs as crumb, i (crumb.label)}
						<Breadcrumb.Item>
							{#if i < crumbs.length - 1 && crumb.href}
								<Breadcrumb.Link href={resolve(crumb.href as '/')} class="text-primary">
									{crumb.label}
								</Breadcrumb.Link>
							{:else if i < crumbs.length - 1}
								<span class="text-muted-foreground">{crumb.label}</span>
							{:else}
								<Breadcrumb.Page class="text-foreground">{crumb.label}</Breadcrumb.Page>
							{/if}
						</Breadcrumb.Item>
						{#if i < crumbs.length - 1}<Breadcrumb.Separator />{/if}
					{/each}
				</Breadcrumb.List>
			</Breadcrumb.Root>
			{#if actions}
				<div class="flex items-center gap-2">{@render actions()}</div>
			{/if}
		</div>
		<div class="ml-auto flex items-center gap-2">
			{@render children?.()}
		</div>
	</div>
</div>

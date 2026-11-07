<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import * as Breadcrumb from '$lib/components/ui/breadcrumb/index.js';
	import { pageTrail } from '../pageTrail.svelte';

	type Crumb = { label: string; href?: string };

	let {
		crumbs,
		actions,
		children
	}: {
		/** Trail from the app down to this page; the last entry is the current page. */
		crumbs: Crumb[];
		/** Primary buttons, shown on the left under the trail (the "New" position). */
		actions?: Snippet;
		/** Right-hand controls: search, filters, view switcher. */
		children?: Snippet;
	} = $props();

	// Tell the shell where this page is; its `breadcrumbs` block, if any, draws the trail.
	$effect(() => {
		const mine = crumbs.map((crumb) => ({ ...crumb }));
		pageTrail.crumbs = mine;
		return () => {
			// The next page may already have published its own trail.
			if (pageTrail.crumbs === mine) pageTrail.crumbs = null;
		};
	});

	/** With a trail in the shell, only the page's own title stays here. */
	const shown = $derived(pageTrail.shellDrawsTrail ? crumbs.slice(-1) : crumbs);
</script>

<!-- Control panel: breadcrumbs, the page's main actions, and its search and view controls. -->
<div class="border-b border-border bg-canvas" data-part="page.control-panel">
	<div class="flex w-full flex-wrap items-center gap-x-6 gap-y-1 px-4 py-2 md:px-6">
		<div class="min-w-0 space-y-1.5">
			<Breadcrumb.Root>
				<Breadcrumb.List class="type-title">
					{#each shown as crumb, i (crumb.label)}
						<Breadcrumb.Item>
							{#if i < shown.length - 1 && crumb.href}
								<Breadcrumb.Link href={resolve(crumb.href as '/')} class="text-primary">
									{crumb.label}
								</Breadcrumb.Link>
							{:else if i < shown.length - 1}
								<span class="text-muted-foreground">{crumb.label}</span>
							{:else}
								<Breadcrumb.Page class="text-foreground">{crumb.label}</Breadcrumb.Page>
							{/if}
						</Breadcrumb.Item>
						{#if i < shown.length - 1}<Breadcrumb.Separator />{/if}
					{/each}
				</Breadcrumb.List>
			</Breadcrumb.Root>
			{#if actions}
				<div class="flex items-center gap-2">{@render actions()}</div>
			{/if}
		</div>
		<div class="ml-auto flex min-w-0 flex-1 items-center justify-end gap-2">
			{@render children?.()}
		</div>
	</div>
</div>

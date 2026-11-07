<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import PageRenderer from '$lib/components/widgets/PageRenderer.svelte';
	import SpinnerWidget from '$lib/components/widgets/SpinnerWidget.svelte';
	import { apiFetch } from '$lib/api/client';
	import { buildPage } from '$lib/dsl';
	import type { PageNode } from '$lib/dsl/types';
	import { PageState, setPageState } from '$lib/pages/pageContext.svelte';
	import { setModalState } from '$lib/components/widgets/modal';

	/**
	 * A setup step (`<page setup_modal="true">`) opened as a dialog on the Apps page: the page's own form, without
	 * leaving the page. It closes when the form has saved, and `onsaved` lets the page ask again what is left to set up.
	 */
	let {
		route,
		title,
		description = null,
		open = $bindable(false),
		onsaved
	}: {
		route: string;
		title: string;
		description?: string | null;
		open: boolean;
		onsaved?: () => void;
	} = $props();

	const pageState = new PageState();
	setPageState(pageState);
	setModalState({
		close: () => {
			open = false;
			onsaved?.();
		},
		editing: null
	});

	let node = $state<PageNode | null>(null);
	let failed = $state(false);

	$effect(() => {
		if (!open || node) return;
		failed = false;
		void (async () => {
			const res = await apiFetch(`/api/ui/pages/${route.replace(/^\/+/, '')}`);
			if (!res.ok) {
				failed = true;
				return;
			}
			const body = (await res.json()) as {
				plugin?: string;
				params?: Record<string, string>;
				page: unknown;
			};
			pageState.plugin = body.plugin ?? '';
			const model = (body.page as { model?: unknown } | undefined)?.model;
			pageState.model = typeof model === 'string' ? model : '';
			pageState.params = body.params ?? {};
			node = buildPage(body.page).page;
		})();
	});
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-4xl">
		<Dialog.Header>
			<Dialog.Title>{title}</Dialog.Title>
			{#if description}
				<Dialog.Description>{description}</Dialog.Description>
			{/if}
		</Dialog.Header>
		<div class="space-y-3 py-2">
			{#if failed}
				<p class="text-sm text-muted-foreground">This step could not be loaded.</p>
			{:else if node}
				{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
					<PageRenderer node={child} />
				{/each}
			{:else}
				<SpinnerWidget node={{ type: 'spinner', label: 'Loading…', inline: true }} />
			{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>

<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import type { ButtonVariant, PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { setModalState } from './modal';
	import { getPageState } from '$lib/pages/pageContext.svelte';

	/**
	 * `<modal label="New channel" title="New channel" variant="outline" size="sm" block="true">`: a button that opens
	 * a wide dialog holding the children (a form, a list). The dialog closes when a form inside it has saved, and when
	 * the page navigates (a form that redirects to the new record).
	 *
	 * With `name="…"`, a list on the same page with `edit="…"` opens it on the row that was clicked: a form inside then
	 * loads that record with its `edit_source` and saves it with its `edit_function`, so one dialog both adds and edits.
	 */
	let { node }: { node: PageNode } = $props();
	let open = $state(false);
	let editing = $state<Record<string, unknown> | null>(null);

	setModalState({
		close: () => (open = false),
		get editing() {
			return editing;
		}
	});
	afterNavigate(() => (open = false));

	const page = getPageState();
	const name = $derived(typeof node.name === 'string' ? node.name.trim() : '');
	$effect(() => {
		if (!page || !name) return;
		const show = (row: Record<string, unknown>) => {
			editing = row;
			open = true;
		};
		page.modals.set(name, show);
		return () => {
			if (page.modals.get(name) === show) page.modals.delete(name);
		};
	});
	// Its own button always adds.
	$effect(() => {
		if (!open) editing = null;
	});
	const title = $derived(
		String((editing ? (node.edit_title ?? node.title) : node.title) ?? node.label ?? 'Dialog')
	);
	/** Each opening starts a fresh form: adding after an edit must not keep the edited values. */
	const editKey = $derived(editing ? String(editing.id ?? '') : '');

	const variant = $derived(
		(typeof node.variant === 'string' ? node.variant : 'outline') as ButtonVariant
	);
	const size = $derived(
		(typeof node.size === 'string' ? node.size : 'sm') as 'default' | 'sm' | 'lg' | 'xs'
	);
	// `width="sm|md|lg|xl"`: how wide the dialog is (default `lg`, 42rem).
	const widths: Record<string, string> = {
		sm: 'sm:max-w-md',
		md: 'sm:max-w-xl',
		lg: 'sm:max-w-2xl',
		xl: 'sm:max-w-4xl'
	};
	const width = $derived(widths[String(node.width ?? 'lg')] ?? widths.lg);
	const block = $derived(node.block === true || node.block === 'true');
</script>

<div>
	<Button
		{size}
		{variant}
		class={block ? 'w-full' : ''}
		onclick={() => {
			editing = null;
			open = true;
		}}
	>
		{String(node.label ?? node.title ?? 'Open')}
	</Button>
</div>

<Dialog.Root bind:open>
	<Dialog.Content class="{width} max-h-[90vh] overflow-y-auto">
		<Dialog.Header>
			<Dialog.Title>{title}</Dialog.Title>
			{#if node.description}
				<Dialog.Description>{String(node.description)}</Dialog.Description>
			{/if}
		</Dialog.Header>
		<div class="space-y-3 py-2">
			{#key editKey}
				{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
					<PageRenderer node={child} />
				{/each}
			{/key}
		</div>
	</Dialog.Content>
</Dialog.Root>

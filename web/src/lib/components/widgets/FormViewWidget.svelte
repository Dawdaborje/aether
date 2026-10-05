<script lang="ts">
	import { goto } from '$app/navigation';
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import ChatterPanel from '$lib/chatter/ChatterPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { navHref } from '$lib/components/layout/nav';
	import {
		FormState,
		getPageState,
		interpolate,
		keyOf,
		setFormState,
		text
	} from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const form = new FormState();
	setFormState(form);

	/// `source`: a function that returns the record to edit (called with the page's route values).
	const source = $derived(text(node.source));
	/// `function`: what Save calls, with the field values and the page's route values.
	const submitFunction = $derived(text(node.function));
	const submitLabel = $derived(text(node.submit) ?? 'Save');
	const successMessage = $derived(text(node.success) ?? 'Saved.');
	/// `redirect="/notes"`: where to go after saving.
	const redirect = $derived(text(node.redirect));
	// A form that creates empties after saving; one that edits a record keeps its values.
	const clearAfter = $derived(node.clear === undefined ? !source : node.clear === true);

	/// `chatter="off"` keeps the panel away from a form even when its model has chatter.
	const chatterOff = $derived(text(node.chatter) === 'off');
	const chatterModel = $derived(text(node.model) ?? page?.model ?? '');
	/// The record being edited: the page's `{id}`, or the id the loaded record came with.
	const recordKey = $derived(
		source ? (page?.params.id ?? (form.values.id ? keyOf(form.values.id) : '')) : ''
	);

	let loading = $state(false);
	let saving = $state(false);
	let failure = $state<string | null>(null);
	let saved = $state<string | null>(null);
	let latest = 0;

	$effect(() => {
		void page?.plugin;
		void source;
		const current = ++latest;
		if (!source || !page) return;
		loading = true;
		void page.call(source, { ...page.params }).then((result) => {
			if (current !== latest) return;
			loading = false;
			if (result.ok && result.data && typeof result.data === 'object' && !Array.isArray(result.data)) {
				form.values = { ...(result.data as Record<string, unknown>) };
				failure = null;
			} else if (!result.ok) {
				failure = result.error;
			}
		});
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!submitFunction || !page || saving) return;
		saving = true;
		failure = null;
		saved = null;
		const result = await page.call(submitFunction, { ...page.params, ...form.values });
		saving = false;
		if (!result.ok) {
			failure = result.error;
			return;
		}
		saved = successMessage;
		if (clearAfter) form.values = {};
		page.refresh();
		if (redirect) await goto(navHref(interpolate(redirect, { ...form.values, ...page.params })));
	}
</script>

<form class="space-y-4" onsubmit={submit} aria-busy={loading}>
	{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
		<PageRenderer node={child} />
	{/each}

	{#if failure}
		<p class="text-sm text-destructive" role="alert">{failure}</p>
	{/if}
	{#if saved}
		<p class="text-sm text-primary" role="status">{saved}</p>
	{/if}
	{#if submitFunction}
		<Button type="submit" disabled={saving || loading}>{saving ? 'Saving…' : submitLabel}</Button>
	{/if}
</form>

{#if page && !chatterOff && chatterModel && recordKey}
	<ChatterPanel
		plugin={page.plugin}
		model={chatterModel}
		{recordKey}
		link={typeof window === 'undefined' ? undefined : window.location.pathname.replace(/^\/web/, '')}
	/>
{/if}

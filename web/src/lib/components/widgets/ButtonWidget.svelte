<script lang="ts">
	import type { ButtonVariant, PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import { goto } from '$app/navigation';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const variant = $derived(
		(node.danger
			? 'destructive'
			: typeof node.variant === 'string'
				? node.variant
				: 'default') as ButtonVariant
	);
	// `{id}` and other route values in an href are filled from the page's own (`/crm/deals/{id}/edit`).
	const href = $derived(
		typeof node.href === 'string' && node.href.trim()
			? navHref(interpolate(node.href.trim(), { ...(page?.params ?? {}) }))
			: null
	);
	const fn = $derived(text(node.function));
	const label = $derived(String(node.label ?? node.name ?? 'Action'));

	let busy = $state(false);
	let error = $state<string | null>(null);

	function saveFile(filename: string, contentType: string, data: string) {
		const binary = atob(data);
		const bytes = new Uint8Array(binary.length);
		for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
		const blob = new Blob([bytes], { type: contentType || 'application/octet-stream' });
		const url = URL.createObjectURL(blob);
		const link = document.createElement('a');
		link.href = url;
		link.download = filename || 'download';
		link.click();
		URL.revokeObjectURL(url);
	}

	async function run() {
		if (!fn || !page || busy) return;
		// `confirm="Mark this deal as lost?"` asks first.
		const question = text(node.confirm);
		if (question && !window.confirm(question)) return;
		busy = true;
		error = null;
		const payload: Record<string, unknown> = { ...page.params };
		// `idparam="lead"` sends the page's record id under the name the function expects.
		const idparam = text(node.idparam);
		if (idparam && page.params.id !== undefined) payload[idparam] = page.params.id;
		const format = text(node.format);
		if (format) payload.format = format;
		const result = await page.call(fn, payload);
		busy = false;
		if (!result.ok) {
			error = result.error ?? 'That did not work.';
			return;
		}
		const file = result.data;
		if (
			file &&
			typeof file === 'object' &&
			'data' in file &&
			typeof (file as { data?: unknown }).data === 'string'
		) {
			const row = file as { filename?: unknown; content_type?: unknown; data: string };
			saveFile(
				typeof row.filename === 'string' ? row.filename : 'report',
				typeof row.content_type === 'string' ? row.content_type : '',
				row.data
			);
			return;
		}
		// An action on a record: show what it changed, or move on (`redirect="/crm/deals"`).
		const redirect = text(node.redirect);
		if (redirect) {
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies the base path
			await goto(navHref(redirect));
		} else {
			page.refresh();
		}
	}
</script>

{#if href}
	<Button size="sm" {variant} {href} data-part="page.button">{label}</Button>
{:else if fn}
	<Button size="sm" {variant} disabled={busy} onclick={run} data-part="page.button">{label}</Button>
	{#if error}
		<p class="text-xs text-destructive" role="alert">{error}</p>
	{/if}
{:else}
	<Button size="sm" {variant}>{label}</Button>
{/if}

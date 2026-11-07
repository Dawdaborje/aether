<script lang="ts">
	import type { ButtonVariant, PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { goto } from '$app/navigation';
	import { navHref } from '$lib/components/layout/nav';
	import { getFormState, getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const form = getFormState();
	/**
	 * `showWhen="status=submitted"`: the button is drawn only while the form's field has that value
	 * (`a=b`, `a!=b`, several separated by commas: any one holding is enough). Without a condition, or
	 * outside a form, it is always drawn.
	 */
	const shown = $derived.by(() => {
		const rule = text(node.showWhen);
		if (!rule || !form) return true;
		return rule.split(',').some((part) => {
			const m = /^\s*([\w.]+)\s*(!=|=)\s*(.*?)\s*$/.exec(part);
			if (!m) return false;
			const actual = String(form.values[m[1]] ?? '').trim();
			return m[2] === '=' ? actual === m[3] : actual !== m[3];
		});
	});
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
			? navHref(interpolate(node.href.trim(), page?.vars ?? {}))
			: null
	);
	const fn = $derived(text(node.function));
	const label = $derived(String(node.label ?? node.name ?? 'Action'));

	let busy = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);

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

	let asking = $state(false);

	function run() {
		if (!fn || !page || busy) return;
		// `confirm="Mark this deal as lost?"` asks first, in a dialog.
		if (text(node.confirm)) asking = true;
		else void execute();
	}

	async function execute() {
		if (!fn || !page || busy) return;
		asking = false;
		busy = true;
		error = null;
		notice = null;
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
		// `{ copy_text: "..." }` puts the text on the clipboard (a link to hand to someone); `copied_notice` is shown after.
		const copy = result.data as { copy_text?: unknown; copied_notice?: unknown } | null | undefined;
		if (typeof copy?.copy_text === 'string') {
			try {
				await navigator.clipboard.writeText(copy.copy_text);
				notice = typeof copy.copied_notice === 'string' ? copy.copied_notice : 'Copied.';
			} catch {
				// The browser refused (no permission, or the click is too old): let the person copy it by hand.
				window.prompt('Copy this text:', copy.copy_text);
			}
			return;
		}
		// `{ open_url: "https://..." }` opens another site (a meeting, an editor) in a new tab; if the browser blocks the
		// tab, this one goes there. Only web addresses.
		const target = (result.data as { open_url?: unknown } | null | undefined)?.open_url;
		if (typeof target === 'string' && /^https?:\/\//i.test(target)) {
			if (!window.open(target, '_blank', 'noopener')) window.location.assign(target);
			return;
		}
		// `{ open_page: "/meet/room/7" }` sends the person to another page of this app.
		const inApp = (result.data as { open_page?: unknown } | null | undefined)?.open_page;
		if (typeof inApp === 'string' && /^\/(?!\/)/.test(inApp)) {
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies the base path
			await goto(navHref(inApp));
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

{#if !shown}
	<!-- hidden by showWhen -->
{:else if href}
	<Button size="sm" {variant} {href} data-part="page.button">{label}</Button>
{:else if fn}
	<Button size="sm" {variant} disabled={busy} onclick={run} data-part="page.button">{label}</Button>
	<AlertDialog.Root bind:open={asking}>
		<AlertDialog.Content>
			<AlertDialog.Header>
				<AlertDialog.Title>{label}</AlertDialog.Title>
				<AlertDialog.Description>{text(node.confirm) ?? ''}</AlertDialog.Description>
			</AlertDialog.Header>
			<AlertDialog.Footer>
				<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
				<AlertDialog.Action onclick={() => void execute()}>Confirm</AlertDialog.Action>
			</AlertDialog.Footer>
		</AlertDialog.Content>
	</AlertDialog.Root>
	<!-- A failed call is shown once, by the error dialog every page has; the button adds no text of its own. -->
	{#if notice}
		<p class="text-xs text-muted-foreground" role="status">{notice}</p>
	{/if}
{:else}
	<Button size="sm" {variant}>{label}</Button>
{/if}

<script lang="ts">
	import { browserStorage } from '$lib/browserStorage';
	import { apiFetch } from '$lib/api/client';
	import { beforeNavigate, goto } from '$app/navigation';
	import { untrack } from 'svelte';
	import { cn } from '$lib/utils';
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import RecordActions from './RecordActions.svelte';
	import ChatterPanel from '$lib/chatter/ChatterPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Alert from '$lib/components/ui/alert';
	import * as Dialog from '$lib/components/ui/dialog';
	import { FieldGroup } from '$lib/components/ui/field';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageChrome } from '$lib/pages/pageChrome.svelte';
	import {
		FormState,
		getFormState,
		getPageState,
		interpolate,
		keyOf,
		setFormState,
		text
	} from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	// Buttons and the state tracker sit in a bar that stays at the top of the sheet, with Save,
	// as a form header: `<header>` (its children), `<statusbar>`, `<button>`,
	// `<buttonbox>` and `<action>` at the top level of the form go there.
	const BAR_TYPES = new Set([
		'statusbar',
		'save',
		'button',
		'buttonbox',
		'action',
		'actions',
		'menuitem'
	]);
	const bar = $derived.by(() => {
		const out: PageNode[] = [];
		for (const child of node.children ?? []) {
			if (child.type === 'header') {
				for (const inner of child.children ?? []) out.push(inner);
			} else if (BAR_TYPES.has(child.type)) {
				out.push(child);
			}
		}
		return out;
	});
	const isBarChild = (child: PageNode) =>
		child.type === 'header' || BAR_TYPES.has(child.type);
	const statusNodes = $derived(bar.filter((child) => child.type === 'statusbar'));
	const actionNodes = $derived(bar.filter((child) => child.type !== 'statusbar'));
	/** A `<save/>` in the bar (from `base_ui.form_bar`) replaces the built-in Save button. */
	const hasSaveNode = $derived(bar.some((child) => child.type === 'save'));

	const page = getPageState();
	const chrome = getPageChrome();

	// The page's main form puts its bar in the page's control panel, under the page name; a
	// form with nowhere to put it (in a pop-up, or a page without a title bar) keeps it at the
	// top of its own sheet.
	const owner = {};
	// A form inside another form (a tab another plugin adds to the page, a pop-up) is not the
	// page's main form and never takes the title bar. Child effects run before their parent's, so
	// without this the nested form would claim the bar first and leave the main form's Save and
	// buttons at the bottom of the sheet.
	const nested = getFormState() !== undefined;
	let claimed = $state(false);
	$effect(() => {
		if (nested) return;
		if (!chrome || !chrome.hasBar || !chrome.barActions) return;
		if (chrome.barOwner !== null) return;
		chrome.barOwner = owner;
		claimed = true;
		return () => {
			if (chrome.barOwner === owner) chrome.barOwner = null;
			claimed = false;
		};
	});

	// Without a title bar (a pop-up form) or when another form has it, Save sits at the bottom
	// of the form. While a page's title bar is still being set up, nothing is shown.
	const inline = $derived(
		!chrome || !chrome.hasBar || nested || (chrome.barOwner !== null && !claimed)
	);

	/** Move `node` into `target` (the control panel) while it is there. */
	function portal(node: HTMLElement, target: HTMLElement | null) {
		const place = (to: HTMLElement | null) => {
			if (to) to.appendChild(node);
		};
		place(target);
		return {
			update: place,
			destroy: () => node.remove()
		};
	}
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

	// Widgets in the bar (`<save/>`) save the form through its state.
	$effect(() => {
		form.saving = saving || loading;
		form.saveLabel = submitLabel;
		form.save = submitFunction ? () => save() : null;
	});

	/** Whether this person has starred the record on screen. */
	let favorite = $state(false);

	$effect(() => {
		const key = recordKey;
		const plugin = page?.plugin;
		const model = chatterModel;
		if (!key || !plugin || !model) {
			favorite = false;
			return;
		}
		void page?.tick;
		void apiFetch(
			`/api/ui/records/favorites?plugin=${encodeURIComponent(plugin)}&model=${encodeURIComponent(model)}`
		)
			.then((res) => (res.ok ? res.json() : { ids: [] }))
			.then((body: { ids?: string[] }) => {
				favorite = (body.ids ?? []).includes(key);
			})
			.catch(() => {
				favorite = false;
			});
	});

	/** Reload the record from the server, dropping unsaved edits. */
	function reloadRecord() {
		loadedFor = '';
		page?.refresh();
	}

	function afterRecordAction(what: 'archive' | 'unarchive' | 'delete' | 'favorite' | 'action') {
		// A deleted record has nothing left to show: go back to where the list is.
		if (what === 'delete' && redirect) {
			void goto(navHref(redirect));
			return;
		}
		reloadRecord();
	}

	/** The source answered, with no record: show the page's `<empty>` block instead of a blank form. */
	let noRecord = $state(false);
	const emptyNode = $derived((node.children ?? []).find((child) => child.type === 'empty'));

	let loading = $state(false);
	let saving = $state(false);
	let failure = $state<string | null>(null);
	let saved = $state<string | null>(null);
	let latest = 0;
	/** Snapshot of last loaded/saved values — live refresh skips when the form is dirty. */
	let baseline = $state('{}');
	/** Last plugin/source/params identity that was loaded (or cleared). */
	let loadedFor = $state('');

	/**
	 * What the form holds, for telling whether it changed: empty values (`''`, `null`) count the
	 * same as missing ones, so a widget that fills in an empty default does not look like an edit.
	 */
	function snapshot(): string {
		const kept: Record<string, unknown> = {};
		for (const key of Object.keys(form.values).sort()) {
			const value = form.values[key];
			if (value === null || value === undefined || value === '') continue;
			kept[key] = value;
		}
		return JSON.stringify(kept);
	}
	const dirty = $derived(snapshot() !== baseline);

	// Unsaved changes: a `*` by the page name, a question before leaving, and drafts kept on
	// this device (localStorage) so an edit survives a reload or a decision to come back later.
	const draftKey = $derived(
		typeof window === 'undefined'
			? ''
			: `aether.draft:${page?.plugin ?? ''}:${window.location.pathname}`
	);
	function readDraft(): { values: Record<string, unknown>; at: number } | null {
		try {
			const raw = draftKey ? browserStorage()?.getItem(draftKey) ?? null : null;
			if (!raw) return null;
			const parsed = JSON.parse(raw) as { values?: Record<string, unknown>; at?: number };
			return parsed.values && typeof parsed.values === 'object'
				? { values: parsed.values, at: Number(parsed.at ?? 0) }
				: null;
		} catch {
			return null;
		}
	}
	function writeDraft() {
		try {
			if (draftKey) browserStorage()?.setItem(draftKey, JSON.stringify({ values: form.values, at: Date.now() }));
		} catch {
			// Storage is full or blocked: the edit simply is not kept.
		}
	}
	function clearDraft() {
		try {
			if (draftKey) browserStorage()?.removeItem(draftKey);
		} catch {
			// Nothing to clear.
		}
	}

	let draftOffer = $state<{ values: Record<string, unknown>; at: number } | null>(null);
	/** Where the person was going when they were asked what to do with their changes. */
	let leaving = $state<string | null>(null);
	let allowLeave = false;

	// After a load (or on a form that starts empty), offer a draft kept earlier.
	$effect(() => {
		void loadedFor;
		void draftKey;
		const found = untrack(() => (dirty ? null : readDraft()));
		draftOffer =
			found && JSON.stringify(found.values) !== untrack(() => snapshot()) ? found : null;
	});

	// The page name shows `*` while this form holds changes.
	$effect(() => {
		if (!chrome || !claimed) return;
		chrome.dirty = dirty;
		return () => {
			chrome.dirty = false;
		};
	});

	beforeNavigate((navigation) => {
		if (allowLeave || !dirty || saving) return;
		if (navigation.type === 'leave') {
			// Closing the tab: keep a draft, and let the browser ask.
			writeDraft();
			navigation.cancel();
			return;
		}
		if (!navigation.to) return;
		navigation.cancel();
		leaving = navigation.to.url.pathname + navigation.to.url.search + navigation.to.url.hash;
	});

	async function leaveTo(where: string) {
		allowLeave = true;
		leaving = null;
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- a path the app already built
		await goto(where);
		allowLeave = false;
	}

	async function saveAndLeave() {
		const where = leaving;
		if (!where) return;
		if (await save({ redirect: false })) await leaveTo(where);
	}

	function keepDraftAndLeave() {
		const where = leaving;
		if (!where) return;
		writeDraft();
		void leaveTo(where);
	}

	function discardAndLeave() {
		const where = leaving;
		if (!where) return;
		clearDraft();
		baseline = snapshot();
		void leaveTo(where);
	}


	// Live `data.changed` matching when `<page model>` is omitted.
	$effect(() => {
		if (!chrome) return;
		return chrome.registerRefreshModel(chatterModel);
	});

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		const paramsKey = JSON.stringify(page?.params ?? {});
		const identity = `${page?.plugin ?? ''}|${source ?? ''}|${paramsKey}`;
		if (!source || !page) return;
		const identityChanged = identity !== loadedFor;
		// Don't clobber in-progress edits on SSE refresh; always load on navigation.
		// `untrack` so typing does not re-run this effect (and cancel in-flight loads).
		if (!identityChanged && untrack(() => snapshot() !== baseline)) {
			return;
		}
		const current = ++latest;
		loading = true;
		void page.call(source, { ...page.params }).then((result) => {
			if (current !== latest) return;
			loading = false;
			noRecord = result.ok && (result.data === null || result.data === undefined);
			if (
				result.ok &&
				result.data &&
				typeof result.data === 'object' &&
				!Array.isArray(result.data)
			) {
				form.values = { ...(result.data as Record<string, unknown>) };
				baseline = snapshot();
				loadedFor = identity;
				failure = null;
			} else if (!result.ok) {
				failure = result.error;
			}
		});
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		await save();
	}

	/** Save the form. Returns whether it worked; `redirect: false` stays on the page. */
	async function save(options: { redirect?: boolean } = {}): Promise<boolean> {
		if (!submitFunction || !page || saving) return false;
		saving = true;
		failure = null;
		saved = null;
		const submitted = { ...page.params, ...form.values };
		const result = await page.call(submitFunction, submitted);
		saving = false;
		if (!result.ok) {
			failure = result.error;
			return false;
		}
		saved = successMessage;
		if (clearAfter) form.values = {};
		baseline = snapshot();
		clearDraft();
		draftOffer = null;
		page.refresh();
		if (redirect && options.redirect !== false) {
			await goto(navHref(interpolate(redirect, submitted)));
		}
		return true;
	}
</script>

<form class="flex w-full flex-col gap-6" onsubmit={submit} aria-busy={loading}>
	{#if loading}
		<p class="text-sm text-muted-foreground">Loading…</p>
	{/if}

	{#if draftOffer}
		<Alert.Root role="status">
			<Alert.Title>Unsaved changes found</Alert.Title>
			<Alert.Description>
				<span>
					You left this form with changes on {new Date(draftOffer.at).toLocaleString()} and kept
					them on this device.
				</span>
				<span class="mt-2 flex gap-2">
					<Button
						type="button"
						size="sm"
						onclick={() => {
							if (draftOffer) form.values = { ...draftOffer.values };
							draftOffer = null;
						}}>Restore</Button
					>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onclick={() => {
							clearDraft();
							draftOffer = null;
						}}>Discard</Button
					>
				</span>
			</Alert.Description>
		</Alert.Root>
	{/if}

	{#if noRecord && emptyNode}
		<PageRenderer node={emptyNode} />
	{:else}
		<FieldGroup class="gap-6">
			{#each node.children ?? [] as child, i (child.name ?? `${child.type}-${i}`)}
				{#if !isBarChild(child) && child.type !== 'empty'}
					<PageRenderer node={child} />
				{/if}
			{/each}
		</FieldGroup>
	{/if}

	{#if (submitFunction || bar.length > 0) && (claimed || inline)}
		<div
			data-part="form.toolbar"
			class={cn(
				// In the title bar the wrapper has no box of its own: only its two parts show.
				claimed
					? 'contents'
					: 'flex flex-wrap items-center justify-between gap-2 border-t border-border pt-3'
			)}
		>
			<div
				class="flex flex-wrap items-center gap-2"
				use:portal={claimed ? (chrome?.barActions ?? null) : null}
			>
				{#if saved}
					<span class="text-xs text-muted-foreground" role="status">{saved}</span>
				{/if}
				{#each actionNodes as child, i (child.name ?? `bar-${child.type}-${i}`)}
					<PageRenderer node={child} />
				{/each}
				{#if source && recordKey && chatterModel}
					<RecordActions
						ids={[recordKey]}
						model={chatterModel}
						{favorite}
						refreshing={loading}
						onRefresh={reloadRecord}
						onDone={afterRecordAction}
					/>
				{/if}
				{#if submitFunction && !hasSaveNode}
					<!-- Not `type="submit"`: in the title bar this button is moved out of the <form>, where a
					     submit button has nothing to submit. Enter in a field still submits the form itself. -->
					<Button type="button" size="sm" disabled={saving || loading} onclick={() => void save()}>
						{saving ? 'Saving…' : submitLabel}
					</Button>
				{/if}
			</div>
			<div
				class="flex flex-wrap items-center gap-2"
				use:portal={claimed ? (chrome?.barStatus ?? null) : null}
			>
				{#each statusNodes as child, i (child.name ?? `status-${i}`)}
					<PageRenderer node={child} />
				{/each}
			</div>
		</div>
	{/if}
</form>

<Dialog.Root open={leaving !== null} onOpenChange={(open) => !open && (leaving = null)}>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Unsaved changes</Dialog.Title>
			<Dialog.Description>
				You changed this form and have not saved it. What should happen to your changes?
			</Dialog.Description>
		</Dialog.Header>
		<Dialog.Footer class="gap-2 sm:justify-between">
			<Button type="button" variant="ghost" onclick={() => (leaving = null)}>Keep editing</Button>
			<span class="flex flex-wrap gap-2">
				<Button type="button" variant="outline" onclick={discardAndLeave}>Discard</Button>
				<Button type="button" variant="outline" onclick={keepDraftAndLeave}>Keep a draft</Button>
				<Button type="button" disabled={!submitFunction || saving} onclick={saveAndLeave}>
					Save
				</Button>
			</span>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

{#if page && !chatterOff && chatterModel && recordKey}
	<div class="mt-8 w-full">
		<ChatterPanel
			plugin={page.plugin}
			model={chatterModel}
			{recordKey}
			link={typeof window === 'undefined'
				? undefined
				: window.location.pathname.replace(/^\/web/, '')}
		/>
	</div>
{/if}

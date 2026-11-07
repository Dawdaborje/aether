<script lang="ts">
	import { pageState } from '$lib/components/layout/composed/pageState.svelte';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { browserStorage } from '$lib/browserStorage';
	import { apiFetch } from '$lib/api/client';
	import { getModalState } from './modal';
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
	const isBarChild = (child: PageNode) => child.type === 'header' || BAR_TYPES.has(child.type);
	const statusNodes = $derived(bar.filter((child) => child.type === 'statusbar'));
	const actionNodes = $derived(bar.filter((child) => child.type !== 'statusbar'));
	/** A `<save/>` in the bar (from `base_ui.form_bar`) replaces the built-in Save button. */
	const hasSaveNode = $derived(bar.some((child) => child.type === 'save'));

	const page = getPageState();
	const modal = getModalState();
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
	// Whether *another* form holds the bar. Tracked on purpose: when plugins reload, the new form mounts
	// while the old one still has the bar, so it must try again the moment the old one lets go (or its
	// buttons stay in the sheet until the page is reloaded). Our own claim does not change this value, so
	// claiming does not re-run the effect.
	const heldByAnother = $derived(
		Boolean(chrome) && chrome!.barOwner !== null && chrome!.barOwner !== owner
	);
	$effect(() => {
		if (nested) return;
		if (!chrome || !chrome.hasBar || !chrome.barActions) return;
		if (heldByAnother) return;
		// `barOwner` is written without tracking: tracked, this effect would depend on the state it
		// writes, re-run, release the bar in its cleanup and claim it again, until Svelte gives up.
		if (untrack(() => chrome.barOwner) !== null) return;
		untrack(() => {
			chrome.barOwner = owner;
		});
		claimed = true;
		return () => {
			untrack(() => {
				if (chrome.barOwner === owner) chrome.barOwner = null;
			});
			claimed = false;
		};
	});

	// Without a title bar (a pop-up form) or when another form has it, Save sits in a sticky bar at
	// the top of the form. While a page's title bar is still being set up, nothing is shown.
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

	/**
	 * In a `<modal name="…">` a list opened on one of its rows, the form edits that row: `edit_source` loads it and
	 * `edit_function` saves it (both get its `id`), in place of the `source` and `function` that add one.
	 */
	const editing = $derived.by(() => {
		const row = modal?.editing;
		const key = row ? keyOf(row.id) : '';
		return key && text(node.edit_function) ? key : '';
	});
	/// `source`: a function that returns the record to edit (called with the page's route values).
	const source = $derived(editing ? text(node.edit_source) : text(node.source));
	/// `function`: what Save calls, with the field values and the page's route values.
	const submitFunction = $derived(editing ? text(node.edit_function) : text(node.function));
	const submitLabel = $derived(
		(editing ? text(node.edit_submit) : undefined) ?? text(node.submit) ?? 'Save'
	);
	const successMessage = $derived(
		(editing ? text(node.edit_success) : undefined) ?? text(node.success) ?? 'Saved.'
	);
	/// `redirect="/notes"`: where to go after saving.
	const redirect = $derived(text(node.redirect));
	// A form that creates empties after saving; one that edits a record keeps its values.
	const clearAfter = $derived(
		!editing && (node.clear === undefined ? !source : node.clear === true)
	);

	/// `chatter="off"` keeps the panel away from a form even when its model has chatter.
	const chatterOff = $derived(text(node.chatter) === 'off');
	const chatterModel = $derived(text(node.model) ?? page?.model ?? '');
	/// The record being edited: the page's `{id}`, or the id the loaded record came with.
	// A row edited in a pop-up has no record bar or chatter there: those belong to a record's own page.
	const recordKey = $derived(
		source && !editing ? (page?.params.id ?? (form.values.id ? keyOf(form.values.id) : '')) : ''
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

	function afterRecordAction(
		what: 'archive' | 'unarchive' | 'delete' | 'undo' | 'favorite' | 'action'
	) {
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
	/** The source failed (a link that is not valid, a record that is gone): there is nothing to fill in. */
	let loadFailed = $state(false);
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
	// Drafts are per page; rows edited in a pop-up would share one, so they keep none.
	const draftKey = $derived(
		typeof window === 'undefined' || editing
			? ''
			: `aether.draft:${page?.plugin ?? ''}:${window.location.pathname}`
	);
	function readDraft(): { values: Record<string, unknown>; at: number } | null {
		try {
			const raw = draftKey ? (browserStorage()?.getItem(draftKey) ?? null) : null;
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
			if (draftKey)
				browserStorage()?.setItem(
					draftKey,
					JSON.stringify({ values: form.values, at: Date.now() })
				);
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
		draftOffer = found && JSON.stringify(found.values) !== untrack(() => snapshot()) ? found : null;
	});

	// The shell's `save-bar` shows while this form holds changes.
	$effect(() => {
		if (!claimed || !dirty) return;
		pageState.form = {
			save: () => void save(),
			discard: () => {
				clearDraft();
				baseline = snapshot();
				window.location.reload();
			},
			get saving() {
				return saving;
			}
		};
		return () => {
			pageState.form = null;
		};
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
		const identity = `${page?.plugin ?? ''}|${source ?? ''}|${paramsKey}|${editing}`;
		if (!source || !page) return;
		const identityChanged = identity !== loadedFor;
		// Don't clobber in-progress edits on SSE refresh; always load on navigation.
		// `untrack` so typing does not re-run this effect (and cancel in-flight loads).
		if (!identityChanged && untrack(() => snapshot() !== baseline)) {
			return;
		}
		const current = ++latest;
		loading = true;
		const args = editing ? { ...page.args, id: editing } : page.args;
		void page.call(source, args).then((result) => {
			if (current !== latest) return;
			loading = false;
			noRecord = result.ok && (result.data === null || result.data === undefined);
			// The source may send the person elsewhere instead (a form that is closed): `{ open_page: "/path" }`.
			const elsewhere = (result.data as { open_page?: unknown } | null | undefined)?.open_page;
			if (result.ok && typeof elsewhere === 'string' && /^\/(?!\/)/.test(elsewhere)) {
				// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies the base path
				void goto(navHref(elsewhere), { replaceState: true });
				return;
			}
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
				loadFailed = false;
			} else if (!result.ok) {
				failure = result.error;
				loadFailed = true;
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
		const submitted = editing
			? { ...page.params, ...form.values, id: editing }
			: { ...page.params, ...form.values };
		const result = await page.call(submitFunction, submitted);
		saving = false;
		if (!result.ok) {
			failure = result.error;
			return false;
		}
		// A function may say what it did: `{ saved_message: "Moved 3 people" }` replaces the page's `success` text.
		const said = (result.data as { saved_message?: unknown } | null | undefined)?.saved_message;
		saved = typeof said === 'string' && said.trim() ? said : successMessage;
		if (clearAfter) form.values = {};
		baseline = snapshot();
		clearDraft();
		draftOffer = null;
		// A function that sends the person elsewhere (`open_url`, `open_page`) leaves nothing here to reload.
		const leavingTo = result.data as { open_url?: unknown; open_page?: unknown } | null | undefined;
		if (
			options.redirect === false ||
			(typeof leavingTo?.open_page !== 'string' && typeof leavingTo?.open_url !== 'string')
		) {
			page.refresh();
		}
		modal?.close();
		// The same `{ open_url }` answer as a button's: another site opens in a new tab (a meeting, an editor).
		const target = (result.data as { open_url?: unknown } | null | undefined)?.open_url;
		if (typeof target === 'string' && /^https?:\/\//i.test(target) && options.redirect !== false) {
			if (!window.open(target, '_blank', 'noopener')) window.location.assign(target);
			return true;
		}
		// `{ open_page: "/meet/room/7" }` sends the person to another page of this app.
		const page_ = (result.data as { open_page?: unknown } | null | undefined)?.open_page;
		if (typeof page_ === 'string' && /^\/(?!\/)/.test(page_) && options.redirect !== false) {
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies the base path
			await goto(navHref(page_));
			return true;
		}
		if (redirect && options.redirect !== false) {
			// What the function answered comes first: a create function says the key of the new record as `id`.
			const answer =
				result.data && typeof result.data === 'object' && !Array.isArray(result.data)
					? (result.data as Record<string, unknown>)
					: {};
			await goto(navHref(interpolate(redirect, { ...submitted, ...answer })));
		}
		return true;
	}
</script>

<!-- Nested forms (a tab inside the employee page, a modal) must not use <form>: HTML forbids a
     form inside a form, and the browser would close the outer one. -->
<svelte:element
	this={nested ? 'div' : 'form'}
	class="flex w-full flex-col gap-6"
	aria-busy={loading}
	{...nested ? {} : { onsubmit: submit }}
>
	{#if (submitFunction || bar.length > 0) && (claimed || inline) && !(noRecord && emptyNode) && !loadFailed}
		<div
			data-part="form.toolbar"
			class={cn(
				// In the title bar the wrapper has no box of its own: only its two parts show.
				claimed
					? 'contents'
					: '-mt-2 flex flex-wrap items-center justify-end gap-2 border-b border-border bg-transparent pt-2 pb-3 [&:not(:has(:not(div)))]:hidden'
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
						history={node.modelHistory === true}
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
		</div>
	{/if}

	<!-- The state steps stay at the top of the form itself; only the buttons go to the title bar. -->
	{#if statusNodes.length > 0 && !(noRecord && emptyNode) && !loadFailed}
		<div data-part="form.status" class="flex flex-wrap items-center justify-end gap-2">
			{#each statusNodes as child, i (child.name ?? `status-${i}`)}
				<PageRenderer node={child} />
			{/each}
		</div>
	{/if}

	{#if loading}
		<p class="text-sm text-muted-foreground">Loading…</p>
	{/if}

	{#if draftOffer}
		<Alert.Root role="status">
			<Alert.Title>Unsaved changes found</Alert.Title>
			<Alert.Description>
				<span>
					You left this form with changes on {new Date(draftOffer.at).toLocaleString()} and kept them
					on this device.
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

	{#if loadFailed}
		<Alert.Root variant="destructive" role="alert">
			<Alert.Title>This page cannot be shown</Alert.Title>
			<Alert.Description>{failure ?? 'It could not be loaded.'}</Alert.Description>
		</Alert.Root>
	{:else if noRecord && emptyNode}
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
</svelte:element>

<Dialog.Root open={leaving !== null} onOpenChange={(open) => !open && (leaving = null)}>
	<Dialog.Content>
		<Dialog.Header class="pt-2">
			<AppIcon name="warning" class="size-6 text-warning" aria-hidden="true" />
			<Dialog.Title>Save your changes?</Dialog.Title>
			<Dialog.Description>
				You have unsaved changes on this form. Choose what to do with them before you leave.
			</Dialog.Description>
		</Dialog.Header>
		<Dialog.Footer
			class="mt-2 flex-col-reverse border-t border-border *:data-[slot=button]:relative *:data-[slot=button]:h-auto *:data-[slot=button]:min-h-16 *:data-[slot=button]:flex-none *:data-[slot=button]:justify-between *:data-[slot=button]:py-4 *:data-[slot=button]:pr-4 sm:flex-col-reverse"
		>
			<Button
				type="button"
				variant="ghost"
				class="border-t border-border"
				onclick={() => (leaving = null)}
			>
				<span class="flex flex-col items-start text-left">
					Keep editing
					<span class="text-xs text-muted-foreground">Stay on this page</span>
				</span>
				<AppIcon name="pencil" />
			</Button>
			<Button
				type="button"
				variant="ghost"
				class="border-t border-border text-destructive hover:bg-destructive hover:text-white"
				onclick={discardAndLeave}
			>
				<span class="flex flex-col items-start text-left">
					Discard changes
					<span class="text-xs opacity-80">Throw them away and leave</span>
				</span>
				<AppIcon name="trash" />
			</Button>
			<Button type="button" variant="secondary" onclick={keepDraftAndLeave}>
				<span class="flex flex-col items-start text-left">
					Keep as draft
					<span class="text-xs opacity-80">Leave now, finish later</span>
				</span>
				<AppIcon name="file-text" />
			</Button>
			<Button type="button" disabled={!submitFunction || saving} onclick={saveAndLeave}>
				<span class="flex flex-col items-start text-left">
					Save and leave
					<span class="text-xs opacity-80">Apply your changes first</span>
				</span>
				<AppIcon name="save" />
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

{#if page && !chatterOff && chatterModel && recordKey}
	<div class="mt-8 w-full">
		<ChatterPanel
			plugin={page.plugin}
			model={chatterModel}
			{recordKey}
			version={page.tick}
			link={typeof window === 'undefined'
				? undefined
				: window.location.pathname.replace(/^\/web/, '')}
		/>
	</div>
{/if}

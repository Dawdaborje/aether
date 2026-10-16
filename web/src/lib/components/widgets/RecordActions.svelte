<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { apiFetch } from '$lib/api/client';
	import { Button } from '$lib/components/ui/button';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import StarIcon from '@lucide/svelte/icons/star';
	import ArchiveIcon from '@lucide/svelte/icons/archive';
	import ArchiveRestoreIcon from '@lucide/svelte/icons/archive-restore';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import {
		callPlugin,
		getPageState,
		pageError,
		text
	} from '$lib/pages/pageContext.svelte';

	/**
	 * What can be done to records, the same in every list and detail page:
	 * - the plugin's own `<action>` buttons (called with each record's `id`),
	 * - the kernel's built-ins: favourite, archive, unarchive and delete,
	 * - refresh.
	 *
	 * `bulk` shows one Actions menu for the selected rows; otherwise the actions are buttons for one record.
	 */
	let {
		ids,
		model,
		extra = [],
		bulk = false,
		favorite = false,
		showRefresh = true,
		refreshing = false,
		onRefresh,
		onDone
	}: {
		/** Record ids the actions apply to (the selection, or the one record on a detail page). */
		ids: string[];
		model: string;
		/** The plugin's own actions from the page's `<actions>`. */
		extra?: PageNode[];
		bulk?: boolean;
		/** Whether the record(s) are favourites now, so the button offers the other direction. */
		favorite?: boolean;
		showRefresh?: boolean;
		/** The data is being reloaded: the Refresh icon spins. */
		refreshing?: boolean;
		onRefresh?: () => void;
		/** Called after an action changed something: reload the data, clear the selection. */
		onDone?: (what: 'archive' | 'unarchive' | 'delete' | 'favorite' | 'action') => void;
	} = $props();

	const page = getPageState();
	const plugin = $derived(page?.plugin ?? '');
	const none = $derived(ids.length === 0);

	let busy = $state(false);
	let confirming = $state<{ title: string; body: string; run: () => Promise<void> } | null>(null);

	function fail(message: string, fn: string) {
		pageError.current = { kind: 'validation', status: 0, message, plugin, fn };
	}

	/** Run one of the kernel's record actions and report what could not be done. */
	async function kernelAction(fn: string, what: 'archive' | 'unarchive' | 'delete') {
		if (none || !model) return;
		busy = true;
		const result = await callPlugin(plugin, fn, { model, ids });
		busy = false;
		confirming = null;
		if (!result.ok) {
			fail(result.error ?? 'The action failed.', fn);
			return;
		}
		const answer = (result.data ?? {}) as { failed?: { id: string; error: string }[] };
		const failed = answer.failed ?? [];
		if (failed.length > 0) {
			const first = failed[0]?.error ?? 'refused';
			fail(
				failed.length === ids.length
					? first
					: `${failed.length} of ${ids.length} could not be done: ${first}`,
				fn
			);
		}
		onDone?.(what);
	}

	async function setFavorite(next: boolean) {
		if (none || !model) return;
		busy = true;
		const res = await apiFetch('/api/ui/records/favorites', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ plugin, model, ids, favorite: next })
		});
		busy = false;
		if (!res.ok) {
			fail('The favourite could not be saved.', 'favorite');
			return;
		}
		onDone?.('favorite');
	}

	async function runExtra(action: PageNode) {
		const fn = text(action.function);
		if (!fn || !page || none) return;
		busy = true;
		let problem: string | null = null;
		for (const id of ids) {
			const result = await page.call(fn, { ...page.params, id });
			if (!result.ok) {
				problem = result.error ?? 'The action failed.';
				break;
			}
		}
		busy = false;
		confirming = null;
		if (problem) fail(problem, fn);
		onDone?.('action');
	}

	function ask(title: string, body: string, run: () => Promise<void>) {
		confirming = { title, body, run };
	}

	const many = $derived(ids.length === 1 ? 'this record' : `${ids.length} records`);

	function clickExtra(action: PageNode) {
		const confirm = text(action.confirm);
		const label = String(action.label ?? action.name ?? 'Action');
		if (confirm) ask(label, confirm, () => runExtra(action));
		else void runExtra(action);
	}

	const askDelete = () =>
		ask('Delete', `Delete ${many}? This cannot be undone.`, () => kernelAction('kernel.delete_records', 'delete'));
	const askArchive = () =>
		ask('Archive', `Archive ${many}? It stays in the database but is hidden from the lists.`, () =>
			kernelAction('kernel.archive_records', 'archive')
		);
</script>

<div class="flex flex-wrap items-center gap-2">
	{#if bulk}
		<DropdownMenu.Root>
			<DropdownMenu.Trigger>
				{#snippet child({ props })}
					<Button {...props} size="sm" variant="outline" disabled={none || busy}>
						Actions
						<ChevronDownIcon class="size-3.5 opacity-70" />
					</Button>
				{/snippet}
			</DropdownMenu.Trigger>
			<DropdownMenu.Content align="start" class="min-w-52">
				{#each extra as action (action.name ?? action.function)}
					<DropdownMenu.Item onclick={() => clickExtra(action)}>
						{String(action.label ?? action.name ?? 'Action')}
					</DropdownMenu.Item>
				{/each}
				{#if extra.length > 0}
					<DropdownMenu.Separator />
				{/if}
				<DropdownMenu.Item onclick={() => setFavorite(true)}>
					<StarIcon class="size-4" /> Add to favorites
				</DropdownMenu.Item>
				<DropdownMenu.Item onclick={() => setFavorite(false)}>
					<StarIcon class="size-4" /> Remove from favorites
				</DropdownMenu.Item>
				<DropdownMenu.Separator />
				<DropdownMenu.Item onclick={askArchive}>
					<ArchiveIcon class="size-4" /> Archive
				</DropdownMenu.Item>
				<DropdownMenu.Item onclick={() => kernelAction('kernel.unarchive_records', 'unarchive')}>
					<ArchiveRestoreIcon class="size-4" /> Unarchive
				</DropdownMenu.Item>
				<DropdownMenu.Separator />
				<DropdownMenu.Item class="text-destructive" onclick={askDelete}>
					<Trash2Icon class="size-4" /> Delete
				</DropdownMenu.Item>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	{:else}
		{#each extra as action (action.name ?? action.function)}
			<Button
				type="button"
				size="sm"
				variant={action.danger ? 'destructive' : 'outline'}
				disabled={none || busy}
				onclick={() => clickExtra(action)}
			>
				{String(action.label ?? action.name ?? 'Action')}
			</Button>
		{/each}
		<Button
			type="button"
			size="sm"
			variant="outline"
			disabled={none || busy}
			onclick={() => setFavorite(!favorite)}
		>
			<StarIcon class="size-4" />
			{favorite ? 'Unfavorite' : 'Favorite'}
		</Button>
		<Button type="button" size="sm" variant="outline" disabled={none || busy} onclick={askArchive}>
			<ArchiveIcon class="size-4" /> Archive
		</Button>
		<Button type="button" size="sm" variant="outline" disabled={none || busy} onclick={askDelete}>
			<Trash2Icon class="size-4" /> Delete
		</Button>
	{/if}
	{#if showRefresh && onRefresh}
		<Button
			type="button"
			size="sm"
			variant="ghost"
			title="Refresh"
			disabled={busy || refreshing}
			aria-busy={refreshing}
			onclick={onRefresh}
		>
			<RefreshCwIcon class={refreshing ? 'size-4 animate-spin' : 'size-4'} />
			<span class="sr-only">Refresh</span>
		</Button>
	{/if}
</div>

<AlertDialog.Root open={confirming !== null} onOpenChange={(open) => !open && (confirming = null)}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{confirming?.title ?? ''}</AlertDialog.Title>
			<AlertDialog.Description>{confirming?.body ?? ''}</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action disabled={busy} onclick={() => confirming && void confirming.run()}>
				Confirm
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

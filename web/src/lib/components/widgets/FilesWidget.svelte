<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { confirmAction } from '$lib/components/dialogs/confirm.svelte';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import type { PageNode } from '$lib/dsl/types';
	import { navHref } from '$lib/components/layout/nav';
	import { Button } from '$lib/components/ui/button';
	import { downloadMedia, formatSize, uploadMedia, type MediaFile } from '$lib/media/api';
	import PreviewPanel from '$lib/media/PreviewPanel.svelte';
	import {
		actionsFor,
		ago,
		iconFor,
		readListing,
		type Action,
		type IconName,
		type Item,
		type Listing
	} from '$lib/pages/files';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';
	import MoveDialog from './files/MoveDialog.svelte';
	import NameDialog from './files/NameDialog.svelte';
	import CommentsDialog from './files/CommentsDialog.svelte';
	import ShareDialog from './files/ShareDialog.svelte';
	import VersionsDialog from './files/VersionsDialog.svelte';

	/**
	 * `<files source="drive_browse" open="/drive/{id}" …/>`: a file browser. `source` answers a listing
	 * (`{ view, folder, path, can_write, folders, files }`); folders open at `open`, files in the viewer panel.
	 * Uploading (the button, or dropping files on the list) uploads through the media store and then calls
	 * `register`. The other attributes name the plugin's functions: `create_folder`, `rename`, `move`, `trash`,
	 * `restore`, `purge`, `empty_trash`, `share`, `unshare`, `shares`, `versions`, `restore_version`, `new_version`,
	 * `tree` (folders to move into), `people` (people to share with), `search`, `favourite` (star or unstar:
	 * `{ kind, id }`), `comments` / `comment` / `delete_comment`, `links` / `create_link` / `revoke_link`
	 * (sharing by link, shown in the share dialog) and `usage` (`{ bytes, quota_bytes }`, shown in the header).
	 * `editor` (an office file opens in the browser's editor: `{ file }` answers `{ open_url }`).
	 * `settings="/drive/settings"` adds a link to a settings page for those whose listing says
	 * `features.settings`. `guard="true"` guards each upload for this page's plugin from the start. An action whose function is
	 * not named is not offered; one the listing's `features` switch off is not either.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const fn = (name: string) => text((node as Record<string, unknown>)[name]);
	const rootLabels: Record<string, string> = {
		mine: 'My files',
		shared: 'Shared with me',
		recent: 'Recent',
		trash: 'Trash',
		favourites: 'Favourites',
		link: 'Shared link',
		search: 'Search results'
	};

	let listing = $state<Listing | null>(null);
	let loading = $state(true);
	let failure = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let busy = $state('');
	let dragging = $state(false);
	let menuFor = $state<string | null>(null);
	let query = $state('');
	let searching = $state(false);
	let picker = $state<HTMLInputElement | null>(null);
	let latest = 0;
	let usage = $state<{ bytes: number; quota_bytes: number } | null>(null);
	const guarded = $derived(text(node.guard) === 'true');

	// dialogs
	let newFolderOpen = $state(false);
	let renameOpen = $state(false);
	let moveOpen = $state(false);
	let shareOpen = $state(false);
	let versionsOpen = $state(false);
	let commentsOpen = $state(false);
	let target = $state<Item | null>(null);
	let previewOpen = $state(false);
	let previewIndex = $state(0);

	const folderId = $derived(page?.params.folder ?? '');
	const view = $derived(searching ? 'search' : (listing?.view ?? 'mine'));
	const canWrite = $derived(listing?.can_write === true && !searching);
	const previewFiles = $derived<MediaFile[]>(
		(listing?.files ?? [])
			.filter((f) => f.media)
			.map((f) => ({
				key: f.media!.key,
				name: f.name,
				mime: f.mime ?? f.media!.mime ?? '',
				size: f.size ?? 0
			}))
	);

	async function load() {
		if (!source || !page) return;
		const current = ++latest;
		loading = listing === null;
		const result = await page.call(source, page.args);
		if (current !== latest) return;
		loading = false;
		if (!result.ok) {
			failure = result.error;
			return;
		}
		failure = null;
		listing = readListing(result.data);
		await loadUsage();
	}

	async function loadUsage() {
		const name = fn('usage');
		if (!name || !page) return;
		const r = await page.call(name, {});
		const d = r.ok ? (r.data as Record<string, unknown>) : null;
		usage = d ? { bytes: Number(d.bytes ?? 0), quota_bytes: Number(d.quota_bytes ?? 0) } : null;
	}

	$effect(() => {
		void page?.plugin;
		void JSON.stringify(page?.args ?? {});
		void page?.tick;
		untrack(() => {
			searching = false;
			query = '';
			void load();
		});
	});

	async function run(
		name: string | undefined,
		payload: Record<string, unknown>,
		done?: string
	): Promise<string | null> {
		if (!name || !page) return 'That is not available here.';
		const r = await page.call(name, payload);
		if (!r.ok) return r.error;
		if (done) notice = done;
		await load();
		return null;
	}

	function go(item: Item) {
		const open = text(node.open);
		if (open) void goto(navHref(interpolate(open, { ...(page?.vars ?? {}), id: item.id })));
	}

	function openItem(item: Item) {
		if (item.kind === 'folder') return go(item);
		const at = previewFiles.findIndex((f) => f.key === item.media?.key);
		if (at >= 0) {
			previewIndex = at;
			previewOpen = true;
		}
	}

	async function download(item: Item) {
		if (!item.media) return;
		try {
			await downloadMedia({
				key: item.media.key,
				name: item.name,
				mime: item.mime ?? '',
				size: item.size ?? 0
			});
		} catch (cause) {
			failure = cause instanceof Error ? cause.message : 'The download failed.';
		}
	}

	async function act(action: Action, item: Item) {
		menuFor = null;
		target = item;
		switch (action.id) {
			case 'open':
				return openItem(item);
			case 'download':
				return void download(item);
			case 'edit_online': {
				// The plugin answers `{ open_url }`: the editor server's address for this file and this person.
				const r = await page?.call(fn('editor')!, { file: item.id });
				const url = (r?.data as { open_url?: unknown } | undefined)?.open_url;
				if (!r?.ok || typeof url !== 'string' || !/^https?:\/\//i.test(url)) {
					failure = r?.error ?? 'The editor could not be opened.';
					return;
				}
				if (!window.open(url, '_blank', 'noopener')) window.location.assign(url);
				return;
			}
			case 'rename':
				renameOpen = true;
				return;
			case 'move':
				moveOpen = true;
				return;
			case 'share':
				shareOpen = true;
				return;
			case 'versions':
				versionsOpen = true;
				return;
			case 'comments':
				commentsOpen = true;
				return;
			case 'favourite':
				failure = await run(
					fn('favourite'),
					{ kind: item.kind, id: item.id, on: !item.favourite },
					item.favourite ? 'Removed from favourites.' : 'Added to favourites.'
				);
				return;
			case 'trash':
				failure = await run(
					fn('trash'),
					{ kind: item.kind, id: item.id },
					`“${item.name}” is in the trash.`
				);
				return;
			case 'restore':
				failure = await run(
					fn('restore'),
					{ kind: item.kind, id: item.id },
					`“${item.name}” is back.`
				);
				return;
			case 'purge':
				if (
					!(await confirmAction({
						title: `Delete “${item.name}” for good?`,
						description: 'This cannot be undone.',
						confirmLabel: 'Delete for good',
						destructive: true
					}))
				)
					return;
				failure = await run(fn('purge'), { kind: item.kind, id: item.id }, 'Deleted.');
				return;
		}
	}

	async function emptyTrash() {
		const sure = await confirmAction({
			title: 'Empty the trash?',
			description: 'Everything in the trash is deleted for good. This cannot be undone.',
			confirmLabel: 'Empty trash',
			destructive: true
		});
		if (!sure) return;
		failure = await run(fn('empty_trash'), {}, 'The trash is empty.');
	}

	async function upload(files: FileList | File[] | null) {
		const picked = Array.from(files ?? []);
		if (picked.length === 0 || !fn('register') || !page) return;
		const errors: string[] = [];
		for (const [i, file] of picked.entries()) {
			busy = `Uploading ${i + 1} of ${picked.length}…`;
			try {
				const media = await uploadMedia(file, guarded ? page.plugin : undefined);
				const r = await page.call(fn('register')!, {
					...(folderId ? { folder: folderId } : {}),
					media
				});
				if (!r.ok) errors.push(`${file.name}: ${r.error}`);
			} catch (cause) {
				errors.push(
					`${file.name}: ${cause instanceof Error ? cause.message : 'the upload failed'}`
				);
			}
		}
		busy = '';
		if (picker) picker.value = '';
		failure = errors.length > 0 ? errors.join(' ') : null;
		if (errors.length < picked.length)
			notice = `Uploaded ${picked.length - errors.length} file${picked.length - errors.length === 1 ? '' : 's'}.`;
		await load();
	}

	async function doSearch() {
		const fnName = fn('search');
		if (!fnName || !page || query.trim().length < 2) return;
		// A listing still on its way must not replace the results.
		latest++;
		const r = await page.call(fnName, { q: query.trim() });
		loading = false;
		if (r.ok) {
			listing = readListing(r.data);
			searching = true;
		} else failure = r.error;
	}

	function clearSearch() {
		query = '';
		searching = false;
		void load();
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') menuFor = null;
	}

	const icons: Record<IconName, string> = {
		folder: 'folder',
		image: 'image',
		pdf: 'file-text',
		sheet: 'table',
		slides: 'presentation',
		doc: 'file-text',
		archive: 'archive',
		audio: 'music',
		video: 'video',
		text: 'file-text',
		file: 'file-text'
	};
	const rows = $derived<Item[]>([...(listing?.folders ?? []), ...(listing?.files ?? [])]);
</script>

<svelte:window {onkeydown} />

<section
	class="relative flex min-h-0 flex-1 flex-col"
	data-part="files"
	role="presentation"
	ondragover={(e) => {
		if (canWrite && fn('register')) {
			e.preventDefault();
			dragging = true;
		}
	}}
	ondragleave={(e) => {
		if (e.currentTarget === e.target) dragging = false;
	}}
	ondrop={(e) => {
		e.preventDefault();
		dragging = false;
		if (canWrite) void upload(e.dataTransfer?.files ?? null);
	}}
>
	<header class="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-3 py-2">
		<nav
			aria-label="Location"
			class="flex min-w-0 flex-1 flex-wrap items-center gap-1 text-sm"
			data-part="breadcrumbs"
		>
			<span class="font-medium">{rootLabels[view] ?? 'Files'}</span>
			{#if !searching}
				{#each listing?.path ?? [] as crumb (crumb.id)}
					<span class="text-muted-foreground">/</span>
					<a
						class="max-w-48 truncate hover:underline"
						href={navHref(interpolate(text(node.open) ?? '/{id}', { id: crumb.id }))}
						>{crumb.name}</a
					>
				{/each}
			{/if}
		</nav>
		{#if listing?.features.settings && text(node.settings)}
			<a
				class="text-xs text-muted-foreground hover:text-foreground hover:underline"
				href={navHref(text(node.settings)!)}
				data-part="files-settings">Settings</a
			>
		{/if}
		{#if usage && fn('usage')}
			<span class="text-xs text-muted-foreground" data-part="usage"
				>{formatSize(usage.bytes)}{usage.quota_bytes > 0
					? ` of ${formatSize(usage.quota_bytes)} used`
					: ' used'}</span
			>
		{/if}
		{#if fn('search')}
			<form
				class="relative"
				onsubmit={(e) => {
					e.preventDefault();
					void doSearch();
				}}
			>
				<AppIcon
					name="search"
					class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
				/>
				<input
					bind:value={query}
					type="search"
					placeholder="Search files"
					aria-label="Search files"
					class="h-8 w-44 rounded-md border border-input bg-transparent pr-2 pl-7 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
					oninput={() => query === '' && searching && clearSearch()}
				/>
			</form>
		{/if}
		{#if canWrite}
			{#if fn('create_folder')}
				<Button size="sm" variant="outline" onclick={() => (newFolderOpen = true)}
					><AppIcon name="folder-add" /> New folder</Button
				>
			{/if}
			{#if fn('register')}
				<input
					bind:this={picker}
					type="file"
					multiple
					class="sr-only"
					onchange={(e) => void upload(e.currentTarget.files)}
				/>
				<Button size="sm" onclick={() => picker?.click()}><AppIcon name="upload" /> Upload</Button>
			{/if}
		{/if}
		{#if view === 'trash' && fn('empty_trash') && rows.length > 0}
			<Button size="sm" variant="outline" onclick={() => void emptyTrash()}>Empty trash</Button>
		{/if}
	</header>

	{#if busy}<p class="shrink-0 px-3 py-1 text-xs text-muted-foreground" role="status">
			{busy}
		</p>{/if}
	{#if failure}<p class="shrink-0 px-3 py-1 text-sm text-destructive" role="alert">
			{failure}
		</p>{/if}
	{#if notice && !failure}<p
			class="shrink-0 px-3 py-1 text-xs text-muted-foreground"
			role="status"
			data-part="notice"
		>
			{notice}
		</p>{/if}

	<div
		class="min-h-0 flex-1 overflow-y-auto {dragging
			? 'bg-primary/5 outline-2 -outline-offset-2 outline-primary outline-dashed'
			: ''}"
	>
		{#if loading}
			<p class="p-4 text-sm text-muted-foreground">Opening…</p>
		{:else if rows.length === 0}
			<p class="p-8 text-center text-sm text-muted-foreground" data-part="files-empty">
				{#if view === 'trash'}The trash is empty.{:else if view === 'shared'}Nothing has been shared
					with you.{:else if searching}Nothing matches that.{:else if canWrite}This folder is empty.
					Drop files here or use Upload.{:else}Nothing here.{/if}
			</p>
		{:else}
			<table class="w-full text-sm" data-part="files-table">
				<thead class="sticky top-0 bg-background text-left text-xs text-muted-foreground">
					<tr>
						<th class="px-3 py-1.5 font-medium">Name</th>
						<th class="hidden px-3 py-1.5 font-medium md:table-cell">Owner</th>
						<th class="hidden px-3 py-1.5 font-medium sm:table-cell">Modified</th>
						<th class="hidden px-3 py-1.5 text-right font-medium sm:table-cell">Size</th>
						<th class="w-10"></th>
					</tr>
				</thead>
				<tbody>
					{#each rows as item (item.kind + item.id)}
						{@const icon = icons[iconFor(item)]}
						<tr
							class="group relative border-t border-border hover:bg-muted/50"
							data-part="file-row"
							data-kind={item.kind}
							data-name={item.name}
						>
							<td class="px-3 py-2">
								<button
									type="button"
									class="flex w-full min-w-0 items-center gap-2 text-left"
									onclick={() => openItem(item)}
								>
									<AppIcon
										name={icon}
										class="size-4 shrink-0 {item.kind === 'folder'
											? 'text-primary'
											: 'text-muted-foreground'}"
									/>
									<span class="truncate">{item.name}</span>
									{#if item.favourite}<AppIcon
											name="star"
											class="size-3.5 shrink-0 fill-current text-amber-500"
											aria-label="Favourite"
										/>{/if}
									{#if item.found_in === 'text'}<span class="shrink-0 text-xs text-muted-foreground"
											>in the text</span
										>{/if}
									{#if item.shared}<AppIcon
											name="users"
											class="size-3.5 shrink-0 text-muted-foreground"
											aria-label="Shared"
										/>{/if}
									{#if item.kind === 'file' && (item.version ?? 1) > 1}<span
											class="shrink-0 text-xs text-muted-foreground">v{item.version}</span
										>{/if}
								</button>
							</td>
							<td class="hidden truncate px-3 py-2 text-muted-foreground md:table-cell"
								>{item.owner_name ?? ''}</td
							>
							<td class="hidden px-3 py-2 whitespace-nowrap text-muted-foreground sm:table-cell"
								>{ago(item.modified_at)}</td
							>
							<td
								class="hidden px-3 py-2 text-right whitespace-nowrap text-muted-foreground sm:table-cell"
								>{item.kind === 'file' ? formatSize(item.size ?? 0) : ''}</td
							>
							<td class="relative px-1 py-1">
								<button
									type="button"
									class="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
									aria-label="Actions for {item.name}"
									aria-haspopup="menu"
									aria-expanded={menuFor === item.kind + item.id}
									onclick={() =>
										(menuFor = menuFor === item.kind + item.id ? null : item.kind + item.id)}
								>
									<AppIcon name="more-horizontal" class="size-4" />
								</button>
								{#if menuFor === item.kind + item.id}
									<div
										class="absolute top-full right-1 z-20 min-w-40 rounded-md border border-border bg-background p-1 shadow-md"
										role="menu"
										data-part="file-menu"
									>
										{#each actionsFor(item, view)
											.filter((a) => a.id !== 'edit_online' || fn('editor'))
											.filter((a) => a.id !== 'rename' || fn('rename'))
											.filter((a) => a.id !== 'move' || fn('move'))
											.filter((a) => a.id !== 'share' || fn('share'))
											.filter((a) => a.id !== 'versions' || fn('versions'))
											.filter((a) => a.id !== 'favourite' || fn('favourite'))
											.filter((a) => a.id !== 'comments' || (fn('comments') && listing?.features.comments !== false))
											.filter((a) => a.id !== 'trash' || fn('trash')) as action (action.id)}
											<button
												type="button"
												role="menuitem"
												class="block w-full rounded px-2 py-1.5 text-left text-sm hover:bg-muted"
												onclick={() => void act(action, item)}>{action.label}</button
											>
										{/each}
									</div>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</div>
	{#if dragging}
		<div
			class="pointer-events-none absolute inset-0 grid place-items-center text-sm font-medium text-primary"
			aria-hidden="true"
		>
			Drop to upload
		</div>
	{/if}
</section>

<NameDialog
	bind:open={newFolderOpen}
	title="New folder"
	submitLabel="Create"
	onsubmit={async (name) =>
		run(fn('create_folder'), { name, ...(folderId ? { parent: folderId } : {}) })}
/>
{#if target}
	<NameDialog
		bind:open={renameOpen}
		title="Rename"
		initial={target.name}
		onsubmit={async (name) => run(fn('rename'), { kind: target!.kind, id: target!.id, name })}
	/>
	<MoveDialog
		bind:open={moveOpen}
		tree={fn('tree')}
		exclude={target.kind === 'folder' ? target.id : ''}
		onsubmit={async (to) =>
			run(fn('move'), { kind: target!.kind, id: target!.id, ...(to ? { to } : {}) }, 'Moved.')}
	/>
	{#if fn('share') && fn('unshare') && fn('shares')}
		<ShareDialog
			bind:open={shareOpen}
			kind={target.kind}
			id={target.id}
			name={target.name}
			shares={fn('shares')!}
			share={fn('share')!}
			unshare={fn('unshare')!}
			people={fn('people')}
			links={listing?.features.links === false ? undefined : fn('links')}
			createLink={fn('create_link')}
			revokeLink={fn('revoke_link')}
			onchange={() => void load()}
		/>
	{/if}
	{#if target.kind === 'file' && fn('versions')}
		<VersionsDialog
			bind:open={versionsOpen}
			file={target.id}
			name={target.name}
			versions={fn('versions')!}
			restore={fn('restore_version')}
			newVersion={fn('new_version')}
			canEdit={target.level === 'edit' || target.level === 'owner'}
			guard={guarded}
			onchange={() => void load()}
		/>
	{/if}
	{#if target.kind === 'file' && fn('comments')}
		<CommentsDialog
			bind:open={commentsOpen}
			file={target.id}
			name={target.name}
			comments={fn('comments')!}
			comment={fn('comment')}
			remove={fn('delete_comment')}
		/>
	{/if}
{/if}
<PreviewPanel files={previewFiles} bind:index={previewIndex} bind:open={previewOpen} />

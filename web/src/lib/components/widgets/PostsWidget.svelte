<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { confirmAction } from '$lib/components/dialogs/confirm.svelte';
	import { tick, untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import { downloadMedia, filesOf, formatSize, viewKind, type MediaFile } from '$lib/media/api';
	import { fileTag } from '$lib/media/fileTag';
	import MediaThumb from './MediaThumb.svelte';
	import ComposerWidget from './ComposerWidget.svelte';
	import PreviewPanel from '$lib/media/PreviewPanel.svelte';
	import { useLiveTopic } from '$lib/pages/liveTopic.svelte';
	import { prependRows, removeRow, rowKey, upsertRows, type Row } from '$lib/pages/messages';
	import { getPageState, rowsOf, text } from '$lib/pages/pageContext.svelte';
	import { splitMentions } from '$lib/pages/mentions';
	import { QUICK_REACTIONS, chipsOf } from '$lib/pages/reactions';

	/**
	 * `<posts source="channel_posts" replies="thread_messages" topic="channel.{channel}" send="post_message"/>`: a
	 * channel drawn as posts, the way a team workspace does: each post is a card with its author, an optional
	 * subject and body, and its replies under it, with a box to reply in place. A new post is started from the
	 * bar at the bottom. (A chat or a group is a stream of messages: `<messages>`.)
	 *
	 * `source` is called with the page's values and `{ limit }` or `{ limit, before }` and answers the posts
	 * oldest first, or `{ rows, more }`. A post carries `reply_count` and `latest_replies` (its newest replies, oldest
	 * first); `replies` (called with `{ thread }`) answers all of a post's replies, the post first. A post published on
	 * `topic` with `{ row }` is added or replaces the one with that id; `deleted` with `{ id }` removes it. `send`,
	 * `react`, `edit`, `delete`, `pin`, `emoji`, `gif`, `attach` and `mentions` are as for `<messages>` and `<composer>`.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const repliesFn = $derived(text(node.replies));
	const reactFn = $derived(text(node.react));
	const editFn = $derived(text(node.edit));
	const deleteFn = $derived(text(node.delete));
	const pinFn = $derived(text(node.pin));
	const limit = $derived(Math.min(50, Math.max(5, Number(node.limit) || 20)));
	const me = $derived(page?.vars.me ?? '');
	const userKey = (user: string) => user.replace(/^[^:]*:/, '');
	const isMine = (row: Row) => me !== '' && userKey(String(row.author ?? '')) === me;

	let posts = $state<Row[]>([]);
	let more = $state(false);
	let loading = $state(false);
	let loadingOlder = $state(false);
	let failure = $state<string | null>(null);
	let actionError = $state<string | null>(null);
	let scroller = $state<HTMLElement | null>(null);
	let atBottom = $state(true);
	/** Every reply of a post, once asked for ("View earlier replies"). */
	let allReplies = $state<Record<string, Row[]>>({});
	let loadingReplies = $state<Record<string, boolean>>({});
	let replying = $state<string | null>(null);
	let composing = $state(false);
	let paletteFor = $state<string | null>(null);
	let editing = $state<string | null>(null);
	let draft = $state('');
	let previewFiles = $state<MediaFile[]>([]);
	let previewIndex = $state(0);
	let previewOpen = $state(false);
	let latest = 0;

	useLiveTopic(() => node.topic, {
		onMessage: (message) => receive(message.event, message.payload),
		onResync: () => void loadNewest(false)
	});

	const composerNode = $derived({
		...node,
		type: 'composer',
		subject: 'true',
		submit: text(node.submit) ?? 'Post',
		placeholder: text(node.placeholder) ?? 'Write a post…',
		typing: 'false'
	} as PageNode);
	const replyNode = $derived({
		...node,
		type: 'composer',
		subject: 'false',
		submit: 'Reply',
		placeholder: 'Write a reply…',
		typing: 'false',
		reply: 'false'
	} as PageNode);

	const time = (row: Row) => {
		const t = Date.parse(String(row.at ?? ''));
		return Number.isNaN(t)
			? ''
			: new Date(t).toLocaleString(undefined, {
					month: 'short',
					day: 'numeric',
					hour: '2-digit',
					minute: '2-digit'
				});
	};
	const initial = (row: Row) =>
		String(row.author_name ?? '?')
			.trim()
			.charAt(0)
			.toUpperCase() || '?';
	const hue = (row: Row) => {
		let h = 0;
		for (const c of String(row.author ?? row.author_name ?? ''))
			h = (h * 31 + c.charCodeAt(0)) % 360;
		return h;
	};

	async function toBottom() {
		await tick();
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
		atBottom = true;
	}

	function moreOf(data: unknown, got: number): boolean {
		if (data && typeof data === 'object' && !Array.isArray(data) && 'more' in data) {
			return (data as { more?: unknown }).more === true;
		}
		return got >= limit;
	}

	async function loadNewest(reset: boolean) {
		if (!source || !page) {
			posts = [];
			return;
		}
		const current = ++latest;
		if (reset) {
			posts = [];
			allReplies = {};
			more = false;
			failure = null;
			replying = null;
			composing = false;
		}
		loading = reset;
		const result = await page.call(source, { ...page.args, limit });
		if (current !== latest) return;
		loading = false;
		if (!result.ok) {
			failure = result.error;
			return;
		}
		failure = null;
		const fresh = rowsOf(result.data) as Row[];
		posts = reset ? fresh : upsertRows(posts, fresh, 'id');
		if (reset) {
			more = moreOf(result.data, fresh.length);
			await toBottom();
		}
	}

	async function loadOlder() {
		if (!source || !page || !more || loadingOlder || posts.length === 0) return;
		loadingOlder = true;
		const result = await page.call(source, { ...page.args, limit, before: rowKey(posts[0], 'id') });
		loadingOlder = false;
		if (!result.ok) {
			actionError = result.error;
			return;
		}
		const older = rowsOf(result.data) as Row[];
		more = moreOf(result.data, older.length);
		// Keep the post being read where it is.
		const before = scroller?.scrollHeight ?? 0;
		posts = prependRows(posts, older, 'id');
		await tick();
		if (scroller) scroller.scrollTop += scroller.scrollHeight - before;
	}

	async function loadReplies(id: string) {
		if (!repliesFn || !page) return;
		loadingReplies = { ...loadingReplies, [id]: true };
		const result = await page.call(repliesFn, { ...page.args, thread: id, limit: 200 });
		loadingReplies = { ...loadingReplies, [id]: false };
		if (!result.ok) {
			actionError = result.error;
			return;
		}
		allReplies = {
			...allReplies,
			[id]: (rowsOf(result.data) as Row[]).filter((r) => rowKey(r, 'id') !== id)
		};
	}

	/** The replies to draw under a post: all of them once asked for, else the newest few. */
	const repliesOf = (post: Row): Row[] =>
		allReplies[rowKey(post, 'id')] ??
		(Array.isArray(post.latest_replies) ? (post.latest_replies as Row[]) : []);
	const hiddenReplies = (post: Row) =>
		allReplies[rowKey(post, 'id')]
			? 0
			: Math.max(0, Number(post.reply_count ?? 0) - repliesOf(post).length);

	function receive(event: string, payload: unknown) {
		const data = payload && typeof payload === 'object' ? (payload as Record<string, unknown>) : {};
		if (event === 'deleted' || event === 'removed') {
			if (data.id !== undefined) posts = removeRow(posts, rowKey({ id: data.id }, 'id'), 'id');
			return;
		}
		if (data.row && typeof data.row === 'object') {
			const row = data.row as Row;
			if (row.parent) return;
			const known = posts.some((p) => rowKey(p, 'id') === rowKey(row, 'id'));
			posts = upsertRows(posts, [row], 'id');
			const id = rowKey(row, 'id');
			if (allReplies[id]) void loadReplies(id);
			if (!known && atBottom) void toBottom();
			return;
		}
		void loadNewest(false);
	}

	function onscroll() {
		if (!scroller) return;
		atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 120;
		if (scroller.scrollTop < 120) void loadOlder();
	}

	/** Put a changed message (a post or a reply) where it is shown. */
	function patch(row: Row) {
		const id = rowKey(row, 'id');
		const parent = row.parent ? rowKey({ id: row.parent }, 'id') : '';
		if (!parent) {
			posts = upsertRows(
				posts,
				[{ ...(posts.find((p) => rowKey(p, 'id') === id) ?? {}), ...row }],
				'id'
			);
			return;
		}
		posts = posts.map((post) => {
			if (rowKey(post, 'id') !== parent) return post;
			const latestReplies = (
				Array.isArray(post.latest_replies) ? (post.latest_replies as Row[]) : []
			).slice();
			const at = latestReplies.findIndex((r) => rowKey(r, 'id') === id);
			if (at >= 0) latestReplies[at] = row;
			return { ...post, latest_replies: latestReplies };
		});
		if (allReplies[parent]) {
			allReplies = { ...allReplies, [parent]: upsertRows(allReplies[parent], [row], 'id') };
		}
	}

	function replied(post: Row, row: Row) {
		const id = rowKey(post, 'id');
		replying = null;
		const next = {
			...post,
			reply_count: Number(post.reply_count ?? 0) + 1,
			latest_replies: [...repliesOf(post), row].slice(allReplies[id] ? undefined : -3)
		};
		posts = upsertRows(posts, [next], 'id');
		if (allReplies[id])
			allReplies = { ...allReplies, [id]: upsertRows(allReplies[id], [row], 'id') };
	}

	async function act(fn: string | undefined, payload: Record<string, unknown>) {
		if (!fn || !page) return;
		actionError = null;
		const result = await page.call(fn, payload);
		if (!result.ok) {
			actionError = result.error;
			return;
		}
		const row = (result.data as { row?: unknown } | null)?.row;
		if (row && typeof row === 'object') patch(row as Row);
	}

	function startEdit(row: Row) {
		editing = rowKey(row, 'id');
		draft = String(row.body ?? '');
		paletteFor = null;
	}

	async function saveEdit(id: string) {
		const body = draft.trim();
		if (!body) return;
		await act(editFn, { id, body });
		editing = null;
	}

	async function remove(row: Row) {
		const isPost = !row.parent;
		const sure = await confirmAction({
			title: isPost ? 'Delete this post?' : 'Delete this reply?',
			description: isPost ? 'The post and all of its replies are removed.' : undefined,
			confirmLabel: 'Delete',
			destructive: true
		});
		if (!sure) return;
		const id = rowKey(row, 'id');
		await act(deleteFn, { id });
		if (isPost) {
			posts = removeRow(posts, id, 'id');
			return;
		}
		const parent = rowKey({ id: row.parent }, 'id');
		posts = posts.map((post) =>
			rowKey(post, 'id') === parent
				? {
						...post,
						reply_count: Math.max(0, Number(post.reply_count ?? 0) - 1),
						latest_replies: ((post.latest_replies as Row[]) ?? []).filter(
							(r) => rowKey(r, 'id') !== id
						)
					}
				: post
		);
		if (allReplies[parent])
			allReplies = { ...allReplies, [parent]: removeRow(allReplies[parent], id, 'id') };
	}

	function open(files: MediaFile[], at: number) {
		previewFiles = files;
		previewIndex = at;
		previewOpen = true;
	}

	// Another channel: start again with its newest posts.
	$effect(() => {
		void page?.plugin;
		void source;
		void JSON.stringify(page?.args ?? {});
		untrack(() => void loadNewest(true));
	});

	// The page asked for fresh data.
	$effect(() => {
		void page?.tick;
		untrack(() => {
			if (posts.length > 0) void loadNewest(false);
		});
	});
</script>

{#snippet message(row: Row, isPost: boolean)}
	{@const id = rowKey(row, 'id')}
	{@const files = filesOf(row.files)}
	{@const chips = reactFn ? chipsOf(row.reactions, me) : []}
	<div
		class="group/message relative flex gap-3"
		data-part={isPost ? 'post' : 'reply'}
		data-message-id={id}
	>
		<span
			class="grid shrink-0 place-items-center rounded-full font-semibold text-white {isPost
				? 'size-10 text-base'
				: 'size-8 text-sm'}"
			style="background: hsl({hue(row)} 45% 42%)"
			aria-hidden="true">{initial(row)}</span
		>
		<div class="min-w-0 flex-1">
			<p class="flex flex-wrap items-baseline gap-x-2">
				<span class="text-sm font-semibold">{row.author_name ?? 'Someone'}</span>
				<span class="text-xs text-muted-foreground">{time(row)}</span>
				{#if row.pinned}<AppIcon
						name="pin"
						class="size-3 self-center fill-current text-muted-foreground"
						aria-label="Pinned"
					/>{/if}
				{#if row.edited_at}<span class="text-xs text-muted-foreground">(edited)</span>{/if}
			</p>
			{#if isPost && row.subject}
				<h3 class="mt-0.5 text-lg leading-snug font-semibold break-words">{row.subject}</h3>
			{/if}
			{#if editing === id}
				<form
					class="mt-1 flex flex-col gap-1"
					onsubmit={(e) => {
						e.preventDefault();
						void saveEdit(id);
					}}
				>
					<textarea
						bind:value={draft}
						rows="3"
						aria-label="Edit"
						class="w-full rounded-md border border-input bg-transparent px-3 py-2 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring"
						onkeydown={(e) => {
							if (e.key === 'Escape') editing = null;
							else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
								e.preventDefault();
								void saveEdit(id);
							}
						}}
					></textarea>
					<span class="text-xs text-muted-foreground">Enter to save, Esc to cancel</span>
				</form>
			{:else}
				<p class="mt-0.5 text-base leading-relaxed break-words whitespace-pre-wrap">
					{#each splitMentions(String(row.body ?? ''), Array.isArray(row.mention_names) ? (row.mention_names as string[]) : []) as part, i (i)}{#if part.mention}<span
								class="rounded bg-primary/10 px-0.5 font-medium text-primary"
								data-part="mention">{part.text}</span
							>{:else}{part.text}{/if}{/each}
				</p>
			{/if}
			{#if files.length > 0}
				<div class="mt-2 flex flex-wrap gap-2">
					{#each files as file, at (file.key)}
						{#if viewKind(file) === 'image'}
							<MediaThumb {file} onopen={() => open(files, at)} />
						{:else}
							{@const tag = fileTag(file)}
							<div
								class="flex w-72 max-w-full items-center gap-3 rounded-lg border border-border bg-background p-2"
							>
								<button
									type="button"
									class="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
									aria-label="Open {file.name}"
									onclick={() => open(files, at)}
								>
									<span
										class="grid size-10 shrink-0 place-items-center rounded-md text-[11px] font-semibold text-white {tag.colour}"
										aria-hidden="true">{tag.label}</span
									>
									<span class="min-w-0 flex-1">
										<span class="block truncate text-sm font-medium" title={file.name}
											>{file.name}</span
										>
										<span class="block text-xs text-muted-foreground"
											>{tag.label} · {formatSize(file.size)}</span
										>
									</span>
								</button>
								<button
									type="button"
									class="grid size-7 shrink-0 place-items-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
									aria-label="Download {file.name}"
									onclick={() => void downloadMedia(file)}
									><AppIcon name="download" class="size-4" /></button
								>
							</div>
						{/if}
					{/each}
				</div>
			{/if}
			{#if chips.length > 0}
				<ul class="mt-2 flex flex-wrap gap-1" data-part="reactions">
					{#each chips as chip (chip.emoji)}
						<li>
							<button
								type="button"
								class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-sm {chip.mine
									? 'border-primary bg-primary/10'
									: 'border-border bg-muted/40 hover:bg-muted'}"
								title={chip.users.map(userKey).join(', ')}
								aria-pressed={chip.mine}
								onclick={() => void act(reactFn, { id, emoji: chip.emoji })}
							>
								<span>{chip.emoji}</span><span>{chip.count}</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
		{#if editing !== id}
			<div
				class="absolute -top-3 right-0 z-10 hidden items-center gap-0.5 rounded-md border border-border bg-background p-0.5 shadow-sm group-focus-within/message:flex group-hover/message:flex"
				data-part="message-actions"
			>
				{#if reactFn}
					<button
						type="button"
						class="rounded p-1 hover:bg-muted"
						aria-label="Add a reaction"
						onclick={() => (paletteFor = paletteFor === id ? null : id)}
					>
						<AppIcon name="face-add" class="size-4" />
					</button>
				{/if}
				{#if isPost}
					<button
						type="button"
						class="rounded p-1 hover:bg-muted"
						aria-label="Reply"
						onclick={() => (replying = id)}
					>
						<AppIcon name="reply" class="size-4" />
					</button>
				{/if}
				{#if pinFn && isPost}
					<button
						type="button"
						class="rounded p-1 hover:bg-muted"
						aria-label={row.pinned ? 'Unpin post' : 'Pin post'}
						onclick={() => void act(pinFn, { id })}
					>
						<AppIcon name="pin" class="size-4 {row.pinned ? 'fill-current' : ''}" />
					</button>
				{/if}
				{#if editFn && isMine(row)}
					<button
						type="button"
						class="rounded p-1 hover:bg-muted"
						aria-label="Edit"
						onclick={() => startEdit(row)}
					>
						<AppIcon name="pencil" class="size-4" />
					</button>
				{/if}
				{#if deleteFn && isMine(row)}
					<button
						type="button"
						class="rounded p-1 hover:bg-muted"
						aria-label="Delete"
						onclick={() => void remove(row)}
					>
						<AppIcon name="trash" class="size-4" />
					</button>
				{/if}
			</div>
			{#if paletteFor === id && reactFn}
				<div
					class="absolute -top-14 right-0 z-20 flex gap-1 rounded-md border border-border bg-background p-1 shadow"
					data-part="reaction-palette"
				>
					{#each QUICK_REACTIONS as emoji (emoji)}
						<button
							type="button"
							class="rounded px-1 text-base hover:bg-muted"
							aria-label="React with {emoji}"
							onclick={() => {
								paletteFor = null;
								void act(reactFn, { id, emoji });
							}}>{emoji}</button
						>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
{/snippet}

<div class="relative flex min-h-64 flex-1 flex-col" data-part="posts">
	<div
		bind:this={scroller}
		{onscroll}
		class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain px-5 py-4"
	>
		{#if failure}
			<p class="text-sm text-destructive" role="alert">{failure}</p>
		{:else if loading && posts.length === 0}
			<p class="text-sm text-muted-foreground">Opening…</p>
		{:else if posts.length === 0}
			<p class="py-10 text-center text-sm text-muted-foreground">
				{text(node.empty) ?? 'No posts yet. Start the first one.'}
			</p>
		{/if}
		{#if more}
			<div class="text-center">
				<Button variant="ghost" size="sm" disabled={loadingOlder} onclick={() => void loadOlder()}>
					{loadingOlder ? 'Loading…' : 'Show earlier posts'}
				</Button>
			</div>
		{/if}
		{#each posts as post (rowKey(post, 'id'))}
			{@const id = rowKey(post, 'id')}
			{@const replies = repliesOf(post)}
			{@const hidden = hiddenReplies(post)}
			<article class="rounded-xl border border-border bg-card" data-part="post-card">
				<div class="p-4">{@render message(post, true)}</div>
				{#if replies.length > 0 || hidden > 0}
					<div class="space-y-4 border-t border-border bg-muted/30 px-4 py-3" data-part="replies">
						{#if hidden > 0}
							<Button
								variant="link"
								size="sm"
								class="h-auto p-0"
								disabled={loadingReplies[id]}
								onclick={() => void loadReplies(id)}
							>
								{loadingReplies[id]
									? 'Loading…'
									: `View ${hidden} earlier ${hidden === 1 ? 'reply' : 'replies'}`}
							</Button>
						{/if}
						{#each replies as reply (rowKey(reply, 'id'))}
							{@render message(reply, false)}
						{/each}
					</div>
				{/if}
				<div class="border-t border-border px-4 py-2">
					{#if replying === id}
						<ComposerWidget
							node={replyNode}
							parent={id}
							bare
							onposted={(row) => replied(post, row as Row)}
						/>
						<Button variant="ghost" size="sm" class="mt-1" onclick={() => (replying = null)}
							>Cancel</Button
						>
					{:else}
						<button
							type="button"
							class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground hover:bg-muted"
							onclick={() => (replying = id)}
						>
							<AppIcon name="reply" class="size-4" /> Reply
						</button>
					{/if}
				</div>
			</article>
		{/each}
	</div>
	{#if actionError}<p class="shrink-0 px-5 pb-1 text-xs text-destructive" role="alert">
			{actionError}
		</p>{/if}
	<div class="shrink-0 border-t border-border bg-background p-3">
		{#if composing}
			<ComposerWidget
				node={composerNode}
				bare
				onposted={(row) => {
					composing = false;
					patch(row as Row);
					void toBottom();
				}}
			/>
			<Button variant="ghost" size="sm" class="mt-1" onclick={() => (composing = false)}
				>Cancel</Button
			>
		{:else}
			<Button
				class="h-11 w-full justify-start px-4 text-base font-normal"
				variant="outline"
				onclick={() => (composing = true)}
			>
				{text(node.start) ?? 'Start a new post'}
			</Button>
		{/if}
	</div>
</div>

<PreviewPanel files={previewFiles} bind:index={previewIndex} bind:open={previewOpen} mode="modal" />

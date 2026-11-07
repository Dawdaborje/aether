<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { confirmAction } from '$lib/components/dialogs/confirm.svelte';
	import { tick, untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { downloadMedia, filesOf, formatSize, viewKind, type MediaFile } from '$lib/media/api';
	import { fileTag } from '$lib/media/fileTag';
	import MediaThumb from './MediaThumb.svelte';
	import UserAvatar from '$lib/components/auth/UserAvatar.svelte';
	import PreviewPanel from '$lib/media/PreviewPanel.svelte';
	import { topicName, useLiveTopic } from '$lib/pages/liveTopic.svelte';
	import {
		displayName,
		groupRows,
		prependRows,
		removeRow,
		rowKey,
		trimRows,
		typingText,
		upsertRows,
		type Fields,
		type Row
	} from '$lib/pages/messages';
	import { getPageState, interpolate, rowsOf, text } from '$lib/pages/pageContext.svelte';
	import { offsetsOf, windowOf } from '$lib/pages/virtual';
	import { splitMentions } from '$lib/pages/mentions';
	import { readPeople } from '$lib/pages/presence';
	import { QUICK_REACTIONS, chipsOf } from '$lib/pages/reactions';

	/**
	 * `<messages source="channel_messages" topic="channel.{channel}"/>`: a conversation. It starts at the newest
	 * message, loads older ones when the person scrolls up, adds new ones as they are published on the topic, and
	 * shows who is typing. Rows are drawn as text by the widget; nothing from a message is used as markup.
	 *
	 * The list is virtual: only the rows near the visible part are in the page, with spacers standing in for the
	 * rest, so a conversation of thousands of rows costs about the same as a short one.
	 *
	 * `source` is called with the page's values and `{ limit }` (the newest rows), or `{ limit, before }` where
	 * `before` is the id of the oldest row held. It answers the rows oldest first, or `{ rows, more }`. A message
	 * published on the topic carries `{ row }` (added, or replaced when the id is known) or, for the event
	 * `deleted`, `{ id }`; a message with neither makes the widget ask `source` for the newest rows again.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const limit = $derived(Math.min(200, Math.max(10, Number(node.limit) || 50)));
	const fields: Fields = $derived({
		id: text(node.id_field) ?? 'id',
		author: text(node.author_field) ?? 'author',
		name: text(node.author_name_field) ?? 'author_name',
		image: text(node.author_image_field) ?? 'author_image',
		time: text(node.time_field) ?? 'created_at',
		body: text(node.body_field) ?? 'body',
		files: text(node.files_field) ?? 'files'
	});
	/** `reply="true"`: a Reply button that quotes the message in the `<composer reply="true">` of the same topic. */
	const canReply = $derived(node.reply === true || node.reply === 'true');
	/** Rows carry `reply_to` (a message id) with `reply_to_name` and `reply_to_body`, drawn as a quote above the text. */

	function startReply(row: Row, key: string) {
		const topic = topicName(page, node.topic);
		if (!page || !topic) return;
		page.replyTo = {
			topic,
			id: key,
			name: displayName(row, fields),
			body: String(row[fields.body] ?? '') || 'Attachment'
		};
	}

	/** Scroll to a quoted message when it is among the loaded rows. */
	async function jumpTo(id: string) {
		const at = keys.indexOf(id);
		if (at < 0 || !scroller) return;
		setTop(Math.max(0, offsets[at] - 40));
		await tick();
		const el = scroller.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(id)}"]`);
		el?.animate([{ backgroundColor: 'var(--accent)' }, { backgroundColor: 'transparent' }], {
			duration: 1500
		});
	}
	const showTyping = $derived(node.typing === true || node.typing === 'true');
	const MAX_ROWS = 2000;
	const NEAR = 120;
	const ESTIMATE = 56;

	let rows = $state<Row[]>([]);
	let more = $state(false);
	let loading = $state(false);
	let loadingOlder = $state(false);
	let failure = $state<string | null>(null);
	let unseen = $state(0);
	let atBottom = $state(true);
	let scroller = $state<HTMLElement | null>(null);
	let scrollTop = $state(0);
	let viewport = $state(480);
	let heights = $state<Record<string, number>>({});
	let typing = $state<string[]>([]);
	let previewFiles = $state<MediaFile[]>([]);
	let previewIndex = $state(0);
	let previewOpen = $state(false);
	// Actions on a message: `react`, `edit` and `delete` name functions of the plugin.
	const reactFn = $derived(text(node.react));
	const editFn = $derived(text(node.edit));
	const deleteFn = $derived(text(node.delete));
	const pinFn = $derived(text(node.pin));
	const me = $derived(page?.vars.me ?? '');
	let paletteFor = $state<string | null>(null);
	let editing = $state<string | null>(null);
	let draft = $state('');
	let actionError = $state<string | null>(null);
	const isMine = (row: Row) =>
		me !== '' && String(row[fields.author] ?? '').replace(/^[^:]*:/, '') === me;
	const timers = new Map<string, ReturnType<typeof setTimeout>>();
	let latest = 0;

	const live = useLiveTopic(() => node.topic, {
		onMessage: (message) => receive(message.event, message.payload),
		onResync: () => void loadNewest(false),
		onEvent: (event) => {
			if (event.kind !== 'typing' || !showTyping) return;
			if (!typing.includes(event.user)) typing = [...typing, event.user];
			clearTimeout(timers.get(event.user));
			timers.set(
				event.user,
				setTimeout(() => {
					typing = typing.filter((u) => u !== event.user);
					timers.delete(event.user);
				}, 4000)
			);
		}
	});

	const dayLabel = (time: number) =>
		new Date(time).toLocaleDateString(undefined, {
			weekday: 'long',
			year: 'numeric',
			month: 'long',
			day: 'numeric'
		});
	const items = $derived(groupRows(rows, fields, dayLabel));
	const keys = $derived(items.map((item, i) => item.key || `#${i}`));
	const offsets = $derived(offsetsOf(keys.map((key) => heights[key] ?? ESTIMATE)));
	const win = $derived(windowOf(offsets, scrollTop, viewport));
	const shown = $derived(items.slice(win.start, win.end));

	const clock = (row: Row) => {
		const t = Date.parse(String(row[fields.time] ?? ''));
		return Number.isNaN(t)
			? ''
			: new Date(t).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	};

	// Real heights replace the guesses as rows are drawn. They are applied once per frame, and a list that is
	// at the bottom stays there while they settle.
	let pending: Record<string, number> = {};
	let frame = 0;
	function flush() {
		frame = 0;
		const changed = Object.entries(pending).filter(([key, h]) => heights[key] !== h);
		pending = {};
		if (changed.length === 0) return;
		// The row at the top of the view and how far down it sits, before the heights change.
		const anchorKey = keys[win.start];
		const gap = offsets[win.start] - scrollTop;
		heights = { ...heights, ...Object.fromEntries(changed) };
		void tick().then(() => {
			if (!scroller) return;
			if (atBottom) {
				// A list at the bottom stays there while heights settle.
				setTop(scroller.scrollHeight);
				return;
			}
			// Otherwise the row being read stays where it is, whatever happened to the rows above it.
			const at = anchorKey === undefined ? -1 : keys.indexOf(anchorKey);
			if (at < 0) return;
			const want = offsets[at] - gap;
			if (Math.abs(scroller.scrollTop - want) > 1) setTop(want);
		});
	}
	function measure(element: HTMLElement, key: string) {
		let current = key;
		const observer = new ResizeObserver(() => {
			pending[current] = element.offsetHeight;
			if (!frame) frame = requestAnimationFrame(flush);
		});
		observer.observe(element);
		return {
			update(next: string) {
				current = next;
			},
			destroy() {
				observer.disconnect();
			}
		};
	}

	$effect(() => {
		if (!scroller) return;
		const el = scroller;
		viewport = el.clientHeight;
		const observer = new ResizeObserver(() => (viewport = el.clientHeight));
		observer.observe(el);
		return () => observer.disconnect();
	});

	// Moves the list from code. The scroll events this causes are not the person's, so `onscroll` ignores them:
	// whether the list "is at the bottom" is what the person last chose, not where heights happen to put it.
	let ignoreScroll = false;
	function setTop(value: number) {
		if (!scroller) return;
		ignoreScroll = true;
		scroller.scrollTop = value;
		requestAnimationFrame(() => (ignoreScroll = false));
	}

	function moreOf(data: unknown, got: number): boolean {
		if (data && typeof data === 'object' && !Array.isArray(data) && 'more' in data) {
			return (data as { more?: unknown }).more === true;
		}
		return got >= limit;
	}

	async function toBottom() {
		await tick();
		setTop(scroller?.scrollHeight ?? 0);
		unseen = 0;
		atBottom = true;
		// Heights are guesses until the rows are drawn; once they are, go to the true bottom.
		requestAnimationFrame(() => atBottom && setTop(scroller?.scrollHeight ?? 0));
	}

	/** The newest rows. `reset` starts the list over (another channel); otherwise they are merged in. */
	async function loadNewest(reset: boolean) {
		if (!source || !page) {
			rows = [];
			return;
		}
		const current = ++latest;
		if (reset) {
			rows = [];
			heights = {};
			more = false;
			failure = null;
			unseen = 0;
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
		const before = rows.length;
		rows = reset ? fresh : upsertRows(rows, fresh, fields.id);
		if (reset) more = moreOf(result.data, fresh.length);
		await afterAdd(reset ? fresh.length : rows.length - before);
	}

	async function loadOlder() {
		if (!source || !page || !more || loadingOlder || rows.length === 0) return;
		loadingOlder = true;
		const result = await page.call(source, {
			...page.args,
			limit,
			before: rowKey(rows[0], fields.id)
		});
		loadingOlder = false;
		if (!result.ok) {
			failure = result.error;
			return;
		}
		const older = rowsOf(result.data) as Row[];
		more = moreOf(result.data, older.length);
		// Keep the row being read where it is: note it and how far below the top it sits, then put it back there.
		const anchorKey = keys[win.start];
		const anchorGap = offsets[win.start] - scrollTop;
		rows = prependRows(rows, older, fields.id);
		await tick();
		const at = keys.indexOf(anchorKey);
		if (at >= 0) setTop(offsets[at] - anchorGap);
	}

	/** After rows were added at the end: stay at the bottom when the person is there, else count them. */
	async function afterAdd(added: number) {
		if (rows.length > MAX_ROWS && atBottom) {
			rows = trimRows(rows, MAX_ROWS, 'newest');
			more = true;
		}
		if (added <= 0) return;
		if (atBottom) await toBottom();
		else unseen += added;
	}

	function receive(event: string, payload: unknown) {
		const data = payload && typeof payload === 'object' ? (payload as Record<string, unknown>) : {};
		if (event === 'deleted' || event === 'removed') {
			if (data.id !== undefined) rows = removeRow(rows, rowKey({ id: data.id }, 'id'), fields.id);
			return;
		}
		if (data.row && typeof data.row === 'object') {
			const before = rows.length;
			rows = upsertRows(rows, [data.row as Row], fields.id);
			void afterAdd(rows.length - before);
			return;
		}
		void loadNewest(false);
	}

	function onscroll() {
		if (!scroller) return;
		scrollTop = scroller.scrollTop;
		if (ignoreScroll) return;
		atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < NEAR;
		if (atBottom) unseen = 0;
		if (scroller.scrollTop < NEAR) void loadOlder();
	}

	async function act(fn: string | undefined, payload: Record<string, unknown>) {
		if (!fn || !page) return;
		actionError = null;
		const result = await page.call(fn, payload);
		if (!result.ok) {
			actionError = result.error;
			return;
		}
		// The answer carries the changed row; showing it now means the person need not wait for the live message.
		const row = (result.data as { row?: unknown } | null)?.row;
		if (row && typeof row === 'object') rows = upsertRows(rows, [row as Row], fields.id);
	}

	function startEdit(row: Row, key: string) {
		editing = key;
		draft = String(row[fields.body] ?? '');
		paletteFor = null;
	}

	async function saveEdit(key: string) {
		const body = draft.trim();
		if (!body) return;
		await act(editFn, { id: key, body });
		editing = null;
	}

	async function remove(key: string) {
		const sure = await confirmAction({
			title: 'Delete this message?',
			confirmLabel: 'Delete',
			destructive: true
		});
		if (!sure) return;
		await act(deleteFn, { id: key });
		rows = removeRow(rows, key, fields.id);
	}

	function open(files: MediaFile[], at: number) {
		previewFiles = files;
		previewIndex = at;
		previewOpen = true;
	}

	// Another channel, or other values the page reads: start again with its newest messages.
	$effect(() => {
		void page?.plugin;
		void source;
		void JSON.stringify(page?.args ?? {});
		void limit;
		untrack(() => {
			typing = [];
			void loadNewest(true).then(() => toBottom());
		});
	});

	// A row the composer on this page just sent for this topic appears at once.
	$effect(() => {
		const posted = page?.posted;
		if (!posted) return;
		untrack(() => {
			if (posted.topic === topicName(page, node.topic)) receive('message', { row: posted.row });
		});
	});

	// The page asked for fresh data (something it shows changed).
	$effect(() => {
		void page?.tick;
		untrack(() => {
			if (rows.length > 0) void loadNewest(false);
		});
	});

	// `people="people"`: a function of the plugin that answers `[{ id, name }]` for `{ users: [ids] }`, so "is typing"
	// names a person even when they have not written here yet. People who wrote in this conversation are known already.
	const peopleFn = $derived(text(node.people));
	let known = $state(new Map<string, string>());
	let askedFor = 0;
	const userKey = (user: string) => user.replace(/^[^:]*:/, '');

	$effect(() => {
		const fn = peopleFn;
		const users = typing;
		if (!fn || !page || users.length === 0) return;
		untrack(() => {
			const missing = users.filter(
				(u) =>
					!known.has(userKey(u)) &&
					!rows.some((r) => userKey(String(r[fields.author] ?? '')) === userKey(u))
			);
			if (missing.length === 0) return;
			const current = ++askedFor;
			void page.call(fn, { ...page.args, users: missing }).then((result) => {
				if (current !== askedFor || !result.ok) return;
				const next = new Map(known);
				for (const [key, person] of readPeople(result.data)) next.set(key, person.name);
				known = next;
			});
		});
	});

	const names = $derived(
		typing.map((user) => {
			const key = userKey(user);
			const written = rows.find((r) => userKey(String(r[fields.author] ?? '')) === key);
			return (known.get(key) ?? (written ? displayName(written, fields) : '') ?? '') || key;
		})
	);
</script>

<div class="relative flex min-h-64 flex-1 flex-col" data-part="messages">
	{#if live.denied}
		<p class="p-4 text-sm text-muted-foreground" role="alert">You cannot read this conversation.</p>
	{:else if failure && rows.length === 0}
		<p class="p-4 text-sm text-destructive" role="alert">{failure}</p>
	{:else}
		<div
			bind:this={scroller}
			{onscroll}
			class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 py-3 [overflow-anchor:none]"
			role="log"
			aria-live="polite"
			aria-label={text(node.title) ?? text(node.label) ?? 'Messages'}
		>
			{#if loadingOlder}<p class="py-1 text-center text-xs text-muted-foreground">
					Loading earlier messages…
				</p>
			{:else if more}<p class="py-1 text-center text-xs text-muted-foreground">
					Scroll up for earlier messages
				</p>{/if}
			{#if loading && rows.length === 0}
				<p class="p-4 text-sm text-muted-foreground">Opening…</p>
			{:else if rows.length === 0}
				<p class="p-4 text-center text-sm text-muted-foreground">
					{text(node.empty) ?? 'No messages yet.'}
				</p>
			{/if}
			<div style="height:{win.before}px" aria-hidden="true"></div>
			{#each shown as item, k (keys[win.start + k])}
				{@const files = filesOf(item.row[fields.files])}
				{@const name = displayName(item.row, fields)}
				{@const mine = isMine(item.row)}
				<div use:measure={keys[win.start + k]} data-part="message-row">
					{#if item.day}
						<div class="my-4 flex items-center gap-3 text-sm text-muted-foreground">
							<span class="h-px flex-1 bg-border"></span><span>{item.day}</span><span
								class="h-px flex-1 bg-border"
							></span>
						</div>
					{/if}
					<article
						class="group relative flex {mine ? 'justify-end' : 'justify-start'} {item.head
							? 'pt-4'
							: 'pt-1'}"
						data-part="message"
						data-mine={mine}
						data-message-id={item.key}
					>
						{#if !mine}
							<div class="mr-2 w-8 shrink-0" data-part="message-avatar">
								{#if item.head}
									<UserAvatar
										{name}
										avatarUrl={typeof item.row[fields.image] === 'string'
											? (item.row[fields.image] as string)
											: null}
										class="size-8"
										fallbackClass="text-xs"
									/>
								{/if}
							</div>
						{/if}
						<div
							class="relative max-w-[min(80%,48rem)] min-w-0 rounded-2xl px-4 py-2.5 {mine
								? 'bg-primary/15'
								: 'bg-muted'} {item.head ? (mine ? 'rounded-tr-sm' : 'rounded-tl-sm') : ''}"
							data-part="bubble"
						>
							{#if item.head && !mine}
								<p class="text-sm font-semibold text-primary">{name}</p>
							{/if}
							{#if item.row.reply_to && !item.row.deleted}
								<button
									type="button"
									class="mb-1 flex max-w-full items-baseline gap-2 rounded-md border-l-4 border-primary bg-background/60 px-2 py-0.5 text-left text-xs text-muted-foreground hover:bg-background"
									data-part="reply-quote"
									onclick={() => void jumpTo(String(item.row.reply_to))}
								>
									<span class="shrink-0 font-medium text-foreground"
										>{item.row.reply_to_name || 'Message'}</span
									>
									<span class="truncate">{item.row.reply_to_body}</span>
								</button>
							{/if}
							{#if editing === item.key}
								<form
									class="mt-1 flex flex-col gap-1"
									onsubmit={(e) => {
										e.preventDefault();
										void saveEdit(item.key);
									}}
								>
									<textarea
										bind:value={draft}
										rows="2"
										aria-label="Edit message"
										class="w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring"
										onkeydown={(e) => {
											if (e.key === 'Escape') editing = null;
											else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
												e.preventDefault();
												void saveEdit(item.key);
											}
										}}
									></textarea>
									<span class="text-xs text-muted-foreground">Enter to save, Esc to cancel</span>
								</form>
							{:else}
								<p class="text-base leading-relaxed break-words whitespace-pre-wrap">
									{#each splitMentions(String(item.row[fields.body] ?? ''), Array.isArray(item.row.mention_names) ? (item.row.mention_names as string[]) : []) as part, i (i)}{#if part.mention}<span
												class="rounded bg-primary/10 px-0.5 font-medium text-primary"
												data-part="mention">{part.text}</span
											>{:else}{part.text}{/if}{/each}{#if item.row.edited_at}<span
											class="ml-1 text-xs text-muted-foreground">(edited)</span
										>{/if}
								</p>
							{/if}
							{#if reactFn}
								{@const chips = chipsOf(item.row.reactions, me)}
								{#if chips.length > 0}
									<ul class="mt-1 flex flex-wrap gap-1" data-part="reactions">
										{#each chips as chip (chip.emoji)}
											<li>
												<button
													type="button"
													class="inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs {chip.mine
														? 'border-primary bg-primary/10'
														: 'border-border bg-muted/40 hover:bg-muted'}"
													title={chip.users.map((u) => u.replace(/^[^:]*:/, '')).join(', ')}
													aria-pressed={chip.mine}
													onclick={() => void act(reactFn, { id: item.key, emoji: chip.emoji })}
												>
													<span>{chip.emoji}</span><span>{chip.count}</span>
												</button>
											</li>
										{/each}
									</ul>
								{/if}
							{/if}
							{#if files.length > 0}
								<div class="mt-1 flex flex-wrap gap-1">
									{#each files as file, at (file.key)}
										{#if viewKind(file) === 'image'}
											<MediaThumb {file} onopen={() => open(files, at)} />
										{:else}
											{@const tag = fileTag(file)}
											<div
												class="group/file flex w-72 max-w-full items-center gap-3 rounded-lg border border-border bg-background/60 p-2 hover:bg-background"
												data-part="file-card"
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
													class="grid size-7 shrink-0 place-items-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
													aria-label="Download {file.name}"
													title="Download"
													onclick={() => void downloadMedia(file)}
													><AppIcon name="download" class="size-4" /></button
												>
											</div>
										{/if}
									{/each}
								</div>
							{/if}
							<p class="mt-1 flex items-center justify-end gap-1 text-xs text-muted-foreground">
								{#if item.row.pinned}<AppIcon
										name="pin"
										class="size-3 fill-current"
										aria-label="Pinned"
									/>{/if}{clock(item.row)}
							</p>
							{#if (canReply || reactFn || editFn || deleteFn || pinFn) && editing !== item.key}
								<div
									class="absolute -top-4 {mine
										? 'right-2'
										: 'left-2'} z-10 hidden items-center gap-0.5 rounded-md border border-border bg-background p-0.5 shadow-sm group-focus-within:flex group-hover:flex"
									data-part="message-actions"
								>
									{#if canReply && !item.row.deleted}
										<button
											type="button"
											class="rounded p-1 hover:bg-muted"
											aria-label="Reply"
											onclick={() => startReply(item.row, item.key)}
											><AppIcon name="reply" class="size-4" /></button
										>
									{/if}
									{#if reactFn}
										<button
											type="button"
											class="rounded p-1 hover:bg-muted"
											aria-label="Add a reaction"
											onclick={() => (paletteFor = paletteFor === item.key ? null : item.key)}
											><AppIcon name="face-add" class="size-4" /></button
										>
									{/if}
									{#if pinFn && !item.row.deleted}
										<button
											type="button"
											class="rounded p-1 hover:bg-muted"
											aria-label={item.row.pinned ? 'Unpin message' : 'Pin message'}
											onclick={() => void act(pinFn, { id: item.key })}
											><AppIcon
												name="pin"
												class="size-4 {item.row.pinned ? 'fill-current' : ''}"
											/></button
										>
									{/if}
									{#if editFn && isMine(item.row)}
										<button
											type="button"
											class="rounded p-1 hover:bg-muted"
											aria-label="Edit message"
											onclick={() => startEdit(item.row, item.key)}
											><AppIcon name="pencil" class="size-4" /></button
										>
									{/if}
									{#if deleteFn && isMine(item.row)}
										<button
											type="button"
											class="rounded p-1 hover:bg-muted"
											aria-label="Delete message"
											onclick={() => void remove(item.key)}
											><AppIcon name="trash" class="size-4" /></button
										>
									{/if}
								</div>
								{#if paletteFor === item.key && reactFn}
									<div
										class="absolute -top-14 {mine
											? 'right-2'
											: 'left-2'} z-20 flex gap-1 rounded-md border border-border bg-background p-1 shadow"
										data-part="reaction-palette"
									>
										{#each QUICK_REACTIONS as emoji (emoji)}
											<button
												type="button"
												class="rounded px-1 text-base hover:bg-muted"
												aria-label="React with {emoji}"
												onclick={() => {
													paletteFor = null;
													void act(reactFn, { id: item.key, emoji });
												}}>{emoji}</button
											>
										{/each}
									</div>
								{/if}
							{/if}
						</div>
					</article>
				</div>
			{/each}
			<div style="height:{win.after}px" aria-hidden="true"></div>
		</div>
		{#if actionError}<p class="shrink-0 px-4 text-xs text-destructive" role="alert">
				{actionError}
			</p>{/if}
		<div class="h-5 shrink-0 px-4 text-xs text-muted-foreground" aria-live="polite">
			{typingText(names)}
		</div>
		{#if unseen > 0}
			<button
				type="button"
				class="absolute right-4 bottom-8 inline-flex items-center gap-1 rounded-full bg-primary px-3 py-1 text-xs text-primary-foreground shadow"
				onclick={() => void toBottom()}
			>
				<AppIcon name="arrow-down" class="size-3" />
				{unseen} new {unseen === 1 ? 'message' : 'messages'}
			</button>
		{/if}
	{/if}
</div>

<PreviewPanel files={previewFiles} bind:index={previewIndex} bind:open={previewOpen} mode="modal" />

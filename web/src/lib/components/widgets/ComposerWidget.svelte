<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import ComposerFileCard from './ComposerFileCard.svelte';
	import EmojiPicker from './EmojiPicker.svelte';
	import GifPicker from './GifPicker.svelte';
	import { formatSize, uploadMedia, type MediaFile } from '$lib/media/api';
	import { topicName } from '$lib/pages/liveTopic.svelte';
	import {
		completedShortcode,
		insertAt,
		searchShortcodes,
		shortcodeQuery,
		type EmojiPack
	} from '$lib/pages/emoji';
	import { loadEmoji } from '$lib/pages/emojiRegistry';
	import { loadGifs } from '$lib/pages/gifRegistry';
	import type { Gif } from '$lib/pages/gif';
	import { apiFetch } from '$lib/api/client';
	import { applyMention, mentionQuery } from '$lib/pages/mentions';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';
	import { realtime } from '$lib/realtime/store.svelte';

	/**
	 * `<composer send="post_message" topic="channel.{channel}" typing="true" attach="true"/>`: a message box. Files dropped anywhere on
	 * the window are attached (with `attach="true"`). Enter
	 * sends, Shift+Enter starts a new line. `send` is a function of the plugin, called with the page's values,
	 * `{ body }` and, when files are attached, `{ files: [{ key, name, mime, size }] }` (uploaded through the media
	 * store first). If the function answers with a `row`, the `<messages>` of the same topic show it at once.
	 */
	let {
		node,
		parent = undefined,
		onposted = undefined,
		bare = false
	}: {
		node: PageNode;
		/** The post a reply answers: sent as `parent`. A reply box has no subject, no typing signal and takes no files dropped on the window. */
		parent?: string;
		/** Called with the row the function answered, instead of showing it through the page. */
		onposted?: (row: Record<string, unknown>) => void;
		/** No frame of its own: the box sits inside something else (a reply under a post). */
		bare?: boolean;
	} = $props();

	const page = getPageState();
	const send = $derived(text(node.send));
	const field = $derived(text(node.field) ?? 'body');
	const canAttach = $derived(node.attach === true || node.attach === 'true');
	/** `subject="true"`: a line above the box for the heading of a post, sent as `subject`. */
	const subjectOn = $derived((node.subject === true || node.subject === 'true') && !parent);
	let subject = $state('');
	/** Files dropped anywhere on the window go to the one box that is not a reply. */
	const dropsOn = $derived(canAttach && !parent);
	/**
	 * `emoji="true"`: the emoji of the registry (the `[[emoji_pack]]` of every enabled plugin, see `pages/emoji.ts`);
	 * `emoji="name"` offers only the pack of that name. It adds a button that opens a picker, `:name` suggestions and
	 * `:name:` replacement. The web app has no emoji of its own: when no plugin registers a pack, there is no button.
	 */
	const emojiOn = $derived(
		node.emoji === true ||
			(typeof node.emoji === 'string' && node.emoji !== 'false' && node.emoji !== '')
	);
	const emojiOnly = $derived(
		typeof node.emoji === 'string' && node.emoji !== 'true' ? node.emoji : undefined
	);
	let pack = $state<EmojiPack | null>(null);
	const canEmoji = $derived(pack !== null);
	/**
	 * `gif="true"`: the GIFs of the registry (the `[[gif_pack]]` of every enabled plugin, see `pages/gif.ts`);
	 * `gif="name"` offers only the pack of that name. A button opens a picker, and the GIF chosen is attached to the
	 * message like a file, with the same preview. Like emoji, the web app has none of its own.
	 */
	const gifOn = $derived(
		node.gif === true || (typeof node.gif === 'string' && node.gif !== 'false' && node.gif !== '')
	);
	const gifOnly = $derived(
		typeof node.gif === 'string' && node.gif !== 'true' ? node.gif : undefined
	);
	let gifs = $state<Gif[] | null>(null);
	const canGif = $derived(gifs !== null);
	let gifOpen = $state(false);
	const sendsTyping = $derived((node.typing === true || node.typing === 'true') && !parent);
	const max = $derived(Math.min(20000, Number(node.max) || 4000));

	/** `reply="true"`: a message chosen with Reply in the `<messages>` of the same topic is quoted by what is sent. */
	const canReply = $derived(node.reply === true || node.reply === 'true');
	const replying = $derived(
		canReply && page?.replyTo && page.replyTo.topic === topicName(page, node.topic)
			? page.replyTo
			: null
	);

	let body = $state('');
	let files = $state<MediaFile[]>([]);
	/** Files being uploaded (shown as cards at once), and the object URLs used to preview pictures before and after. */
	let pending = $state<
		{ id: number; name: string; size: number; mime: string; local: string | null }[]
	>([]);
	let previews = $state<Record<string, string>>({});
	let pendingId = 0;

	function forget(urls: string[]) {
		for (const url of urls) URL.revokeObjectURL(url);
	}

	function removeFile(file: MediaFile) {
		const url = previews[file.key];
		if (url) forget([url]);
		const { [file.key]: _gone, ...rest } = previews;
		previews = rest;
		files = files.filter((f) => f.key !== file.key);
	}

	function clearFiles() {
		forget(Object.values(previews));
		previews = {};
		files = [];
	}
	let sending = $state(false);
	let uploading = $state(false);
	let error = $state<string | null>(null);
	let box = $state<HTMLTextAreaElement | null>(null);
	let picker = $state<HTMLInputElement | null>(null);
	// `mentions="list_people"`: typing `@` offers the people the function lists (rows with `id` and `name`).
	const mentionsFn = $derived(text(node.mentions));
	let suggest = $state<{ start: number; query: string } | null>(null);
	let options = $state<{ id: string; name: string }[]>([]);
	let chosen = $state(0);
	let askTimer: ReturnType<typeof setTimeout> | null = null;
	let askCount = 0;
	let lastTyping = 0;
	let posts = 0;

	let emojiOpen = $state(false);

	$effect(() => {
		if (!emojiOn) {
			pack = null;
			return;
		}
		let live = true;
		void loadEmoji(emojiOnly).then((found) => live && (pack = found));
		return () => (live = false);
	});

	$effect(() => {
		if (!gifOn) {
			gifs = null;
			return;
		}
		let live = true;
		void loadGifs(gifOnly).then((found) => live && (gifs = found));
		return () => (live = false);
	});

	// Typing `:smi` offers the emoji named like that; `:smile:` is replaced at once.
	let codeAt = $state<{ start: number; query: string } | null>(null);
	let codeOptions = $state<{ name: string; emoji: string }[]>([]);
	let codeChosen = $state(0);

	function updateCodes() {
		if (!pack || !box) return;
		const caret = box.selectionStart ?? body.length;
		const done = completedShortcode(pack, body, caret);
		if (done) {
			const next = body.slice(0, done.start) + done.emoji + body.slice(caret);
			body = next;
			codeAt = null;
			codeOptions = [];
			const at = done.start + done.emoji.length;
			requestAnimationFrame(() => {
				box?.setSelectionRange(at, at);
				grow();
			});
			return;
		}
		const found = shortcodeQuery(body, caret);
		const matches = found
			? searchShortcodes(pack, found.query).map((x) => ({ name: x.name, emoji: x.emoji.c }))
			: [];
		codeAt = matches.length > 0 ? found : null;
		codeOptions = matches;
		codeChosen = 0;
	}

	function pickCode(emoji: string) {
		if (!codeAt || !box) return;
		const caret = box.selectionStart ?? body.length;
		const next = insertAt(body, codeAt.start, caret, emoji);
		body = next.text;
		codeAt = null;
		codeOptions = [];
		requestAnimationFrame(() => {
			box?.focus();
			box?.setSelectionRange(next.caret, next.caret);
			grow();
		});
	}

	function addEmoji(emoji: string) {
		const at = box?.selectionStart ?? body.length;
		const done = insertAt(body, at, box?.selectionEnd ?? at, emoji);
		body = done.text;
		requestAnimationFrame(() => {
			box?.focus();
			box?.setSelectionRange(done.caret, done.caret);
			grow();
		});
	}

	// Choosing Reply moves the cursor to the box.
	$effect(() => {
		if (replying) untrack(() => box?.focus());
	});

	const ready = $derived((body.trim() !== '' || files.length > 0) && !sending && !uploading);

	function grow() {
		if (!box) return;
		box.style.height = 'auto';
		box.style.height = `${Math.min(box.scrollHeight, 8 * 24)}px`;
	}

	function updateSuggest() {
		if (!mentionsFn || !box || !page) return;
		const found = mentionQuery(body, box.selectionStart ?? body.length);
		suggest = found;
		if (askTimer) clearTimeout(askTimer);
		if (!found) {
			options = [];
			return;
		}
		const ask = ++askCount;
		askTimer = setTimeout(async () => {
			const result = await page.call(mentionsFn, { q: found.query });
			if (ask !== askCount || !result.ok) return;
			const rows = (Array.isArray(result.data) ? result.data : []) as {
				id?: unknown;
				name?: unknown;
			}[];
			const people = rows
				.map((r) => ({ id: String(r.id ?? ''), name: String(r.name ?? '') }))
				.filter((r) => r.name);
			const everyone = 'channel'.startsWith(found.query.toLowerCase())
				? [{ id: 'channel', name: 'channel' }]
				: [];
			options = [...people.slice(0, 6), ...everyone];
			chosen = 0;
		}, 120);
	}

	function pick(name: string) {
		if (!suggest || !box) return;
		const done = applyMention(body, box.selectionStart ?? body.length, suggest.start, name);
		body = done.text;
		suggest = null;
		options = [];
		requestAnimationFrame(() => {
			box?.focus();
			box?.setSelectionRange(done.caret, done.caret);
			grow();
		});
	}

	function typed() {
		grow();
		updateSuggest();
		updateCodes();
		if (!sendsTyping || Date.now() - lastTyping < 2000) return;
		const topic = topicName(page, node.topic);
		if (!topic) return;
		lastTyping = Date.now();
		realtime().sendEvent(topic, 'typing');
	}

	async function submit() {
		if (!ready || !send || !page) return;
		sending = true;
		error = null;
		const payload: Record<string, unknown> = { ...page.args, [field]: body.trim() };
		if (files.length > 0) payload.files = files;
		if (replying) payload.reply_to = replying.id;
		if (parent) payload.parent = parent;
		if (subjectOn && subject.trim()) payload.subject = subject.trim();
		const result = await page.call(send, payload);
		sending = false;
		if (!result.ok) {
			error = result.error;
			return;
		}
		const row = (result.data as { row?: unknown } | null)?.row;
		const topic = topicName(page, node.topic);
		if (onposted && row && typeof row === 'object') onposted(row as Record<string, unknown>);
		else if (topic && row && typeof row === 'object') {
			page.posted = { topic, row: row as Record<string, unknown>, n: ++posts };
		}
		body = '';
		subject = '';
		clearFiles();
		codeAt = null;
		codeOptions = [];
		emojiOpen = false;
		if (page) page.replyTo = null;
		lastTyping = 0;
		requestAnimationFrame(grow);
		box?.focus();
	}

	function onkeydown(event: KeyboardEvent) {
		// While the list of emoji names is open, the keys choose from it.
		if (codeAt && codeOptions.length > 0) {
			if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
				event.preventDefault();
				const step = event.key === 'ArrowDown' ? 1 : -1;
				codeChosen = (codeChosen + step + codeOptions.length) % codeOptions.length;
				return;
			}
			if ((event.key === 'Enter' || event.key === 'Tab') && !event.isComposing) {
				event.preventDefault();
				pickCode(codeOptions[codeChosen].emoji);
				return;
			}
			if (event.key === 'Escape') {
				codeAt = null;
				codeOptions = [];
				return;
			}
		}
		// While the list of people is open, the keys choose from it.
		if (suggest && options.length > 0) {
			if (event.key === 'ArrowDown') {
				event.preventDefault();
				chosen = (chosen + 1) % options.length;
				return;
			}
			if (event.key === 'ArrowUp') {
				event.preventDefault();
				chosen = (chosen - 1 + options.length) % options.length;
				return;
			}
			if ((event.key === 'Enter' || event.key === 'Tab') && !event.isComposing) {
				event.preventDefault();
				pick(options[chosen].name);
				return;
			}
			if (event.key === 'Escape') {
				suggest = null;
				options = [];
				return;
			}
		}
		if (event.key === 'Escape' && replying && page) {
			page.replyTo = null;
			return;
		}
		// Enter sends; Shift+Enter is a line break; a key pressed while composing text (IME) is left alone.
		if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
			event.preventDefault();
			void submit();
		}
	}

	// Files dragged anywhere over the window are picked up, so a conversation takes a file dropped on it.
	let dragging = $state(false);
	let dragDepth = 0;

	const hasFiles = (event: DragEvent) => event.dataTransfer?.types.includes('Files') ?? false;

	function onDragEnter(event: DragEvent) {
		if (!dropsOn || !hasFiles(event)) return;
		event.preventDefault();
		dragDepth++;
		dragging = true;
	}

	function onDragOver(event: DragEvent) {
		if (!dropsOn || !hasFiles(event)) return;
		// Without this the browser would open the file instead of handing it to the page.
		event.preventDefault();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
	}

	function onDragLeave(event: DragEvent) {
		if (!dropsOn || !hasFiles(event)) return;
		dragDepth = Math.max(0, dragDepth - 1);
		if (dragDepth === 0) dragging = false;
	}

	function onDrop(event: DragEvent) {
		if (!dropsOn || !hasFiles(event)) return;
		event.preventDefault();
		dragDepth = 0;
		dragging = false;
		void attach(event.dataTransfer?.files ?? null);
		box?.focus();
	}

	async function attach(list: FileList | null) {
		await attachFiles(Array.from(list ?? []));
	}

	/** A GIF of the picker: fetched like any file, uploaded, and attached with a preview. */
	async function pickGif(gif: Gif) {
		gifOpen = false;
		error = null;
		try {
			const response = await apiFetch(gif.url);
			if (!response.ok) throw new Error(`The GIF could not be loaded (${response.status}).`);
			const blob = await response.blob();
			await attachFiles([new File([blob], `${gif.name}.gif`, { type: 'image/gif' })]);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The GIF could not be loaded.';
		}
		box?.focus();
	}

	async function attachFiles(picked: File[]) {
		if (picked.length === 0) return;
		uploading = true;
		error = null;
		try {
			for (const file of picked) {
				const id = ++pendingId;
				const local = file.type.startsWith('image/') ? URL.createObjectURL(file) : null;
				pending = [...pending, { id, name: file.name, size: file.size, mime: file.type, local }];
				try {
					const stored = await uploadMedia(file);
					if (local) previews = { ...previews, [stored.key]: local };
					files = [...files, stored];
				} catch (cause) {
					if (local) forget([local]);
					error = cause instanceof Error ? cause.message : 'The upload failed.';
				} finally {
					pending = pending.filter((p) => p.id !== id);
				}
			}
		} finally {
			uploading = false;
			if (picker) picker.value = '';
		}
	}
</script>

<svelte:window
	ondragenter={onDragEnter}
	ondragover={onDragOver}
	ondragleave={onDragLeave}
	ondrop={onDrop}
/>

{#if dragging}
	<div
		class="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-background/80"
		data-part="drop-zone"
		role="presentation"
	>
		<p
			class="rounded-md border-2 border-dashed border-primary bg-background px-8 py-6 text-lg font-medium"
		>
			Drop files to attach them
		</p>
	</div>
{/if}

<form
	class="shrink-0 bg-background {bare ? '' : 'border-t border-border p-2'}"
	data-part="composer"
	onsubmit={(event) => {
		event.preventDefault();
		void submit();
	}}
>
	{#if replying}
		<div
			class="mb-2 flex items-center gap-2 border-l-2 border-primary bg-muted/50 px-2 py-1 text-xs"
			data-part="reply-bar"
		>
			<span class="min-w-0 flex-1 truncate"
				><span class="text-muted-foreground">Replying to</span>
				<span class="font-medium">{replying.name}</span>
				<span class="text-muted-foreground">{replying.body}</span></span
			>
			<button
				type="button"
				class="shrink-0 text-muted-foreground hover:text-foreground"
				aria-label="Cancel reply"
				onclick={() => page && (page.replyTo = null)}
				><AppIcon name="close" class="size-3" /></button
			>
		</div>
	{/if}
	{#if files.length > 0 || pending.length > 0}
		<ul class="mb-2 flex flex-wrap gap-2" data-part="composer-files" aria-label="Files to send">
			{#each files as file (file.key)}
				<ComposerFileCard
					name={file.name}
					size={file.size}
					mime={file.mime}
					key={file.key}
					local={previews[file.key] ?? null}
					onremove={() => removeFile(file)}
				/>
			{/each}
			{#each pending as item (item.id)}
				<ComposerFileCard
					name={item.name}
					size={item.size}
					mime={item.mime}
					local={item.local}
					uploading
					onremove={() => {}}
				/>
			{/each}
		</ul>
	{/if}
	{#if subjectOn}
		<input
			bind:value={subject}
			type="text"
			maxlength="200"
			placeholder={text(node.subject_placeholder) ?? 'Add a subject'}
			aria-label="Subject"
			class="mb-2 h-11 w-full rounded-lg border border-input bg-transparent px-4 text-base font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring"
		/>
	{/if}
	<div class="relative flex items-end gap-2">
		{#if codeAt && codeOptions.length > 0}
			<ul
				class="absolute bottom-full left-10 z-20 mb-1 max-h-56 min-w-48 overflow-y-auto rounded-md border border-border bg-background p-1 shadow"
				role="listbox"
				aria-label="Emoji"
				data-part="emoji-suggest"
			>
				{#each codeOptions as { name, emoji }, i (name)}
					<li role="option" aria-selected={i === codeChosen}>
						<button
							type="button"
							class="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-sm {i ===
							codeChosen
								? 'bg-secondary'
								: 'hover:bg-muted'}"
							onmousedown={(e) => {
								e.preventDefault();
								pickCode(emoji);
							}}><span class="text-lg">{emoji}</span><span>:{name}:</span></button
						>
					</li>
				{/each}
			</ul>
		{/if}
		{#if suggest && options.length > 0}
			<ul
				class="absolute bottom-full left-10 z-20 mb-1 max-h-48 min-w-48 overflow-y-auto rounded-md border border-border bg-background p-1 shadow"
				role="listbox"
				aria-label="People"
				data-part="mention-list"
			>
				{#each options as option, i (option.id)}
					<li role="option" aria-selected={i === chosen}>
						<button
							type="button"
							class="w-full rounded px-2 py-1 text-left text-sm {i === chosen
								? 'bg-secondary'
								: 'hover:bg-muted'}"
							onmousedown={(e) => {
								e.preventDefault();
								pick(option.name);
							}}>{option.id === 'channel' ? '@channel: everyone here' : option.name}</button
						>
					</li>
				{/each}
			</ul>
		{/if}
		{#if canEmoji}
			<Button
				type="button"
				size="icon"
				variant="ghost"
				aria-label="Add an emoji"
				aria-expanded={emojiOpen}
				onclick={() => {
					emojiOpen = !emojiOpen;
					gifOpen = false;
				}}
			>
				<AppIcon name="message-circle-heart" />
			</Button>
			{#if emojiOpen && pack}
				<EmojiPicker
					{pack}
					onpick={addEmoji}
					onclose={() => {
						emojiOpen = false;
						box?.focus();
					}}
				/>
			{/if}
		{/if}
		{#if canGif}
			<Button
				type="button"
				size="icon"
				variant="ghost"
				aria-label="Add a GIF"
				aria-expanded={gifOpen}
				disabled={uploading}
				class="text-xs font-semibold"
				onclick={() => {
					gifOpen = !gifOpen;
					emojiOpen = false;
				}}
			>
				GIF
			</Button>
			{#if gifOpen && gifs}
				<GifPicker
					{gifs}
					onpick={(gif) => void pickGif(gif)}
					onclose={() => {
						gifOpen = false;
						box?.focus();
					}}
				/>
			{/if}
		{/if}
		{#if canAttach}
			<Button
				type="button"
				size="icon"
				variant="ghost"
				aria-label="Attach files"
				disabled={uploading}
				onclick={() => picker?.click()}
			>
				<AppIcon name="attachment" />
			</Button>
			<input
				bind:this={picker}
				type="file"
				multiple
				class="sr-only"
				onchange={(e) => void attach(e.currentTarget.files)}
			/>
		{/if}
		<textarea
			bind:this={box}
			bind:value={body}
			rows="1"
			maxlength={max}
			placeholder={text(node.placeholder) ?? 'Write a message…'}
			aria-label={text(node.label) ?? 'Message'}
			class="max-h-56 min-h-11 flex-1 resize-none rounded-lg border border-input bg-transparent px-4 py-2.5 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring"
			oninput={typed}
			{onkeydown}
		></textarea>
		<Button type="submit" size="icon" disabled={!ready} aria-label={text(node.submit) ?? 'Send'}>
			<AppIcon name="send" />
		</Button>
	</div>
	{#if error}<p class="mt-1 text-xs text-destructive" role="alert">{error}</p>{/if}
</form>

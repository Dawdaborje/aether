<script lang="ts">
	import { untrack } from 'svelte';
	import BellIcon from '@lucide/svelte/icons/bell';
	import BellOffIcon from '@lucide/svelte/icons/bell-off';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import LockIcon from '@lucide/svelte/icons/lock';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';
	import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';
	import { timeAgo } from '$lib/notifications/time';
	import {
		changeLines,
		chatterApi,
		type ChatterMessage,
		type ChatterThread,
		type Person
	} from './api';

	let {
		plugin,
		model,
		recordKey,
		link
	}: {
		plugin: string;
		model: string;
		/** The record's key (the id without its table). */
		recordKey: string;
		/** An app path to the record's page, put in the notifications people get. */
		link?: string;
	} = $props();

	let thread = $state<ChatterThread | null>(null);
	let loadError = $state<string | null>(null);
	let showTrash = $state(false);

	let mode = $state<'message' | 'note'>('message');
	let draft = $state('');
	let guestName = $state('');
	let posting = $state(false);
	let postError = $state<string | null>(null);

	/** People picked with `@`, by name as typed. */
	let picked = $state<Record<string, string>>({});
	let suggestions = $state<Person[]>([]);
	let suggestToken = 0;

	let editing = $state<string | null>(null);
	let editDraft = $state('');
	let confirmDelete = $state<string | null>(null);
	let failure = $state<string | null>(null);

	let latest = 0;
	async function load(): Promise<void> {
		const current = ++latest;
		const answer = await chatterApi.thread(plugin, model, recordKey, showTrash);
		if (current !== latest) return;
		if (!answer.ok) {
			// A private or missing record shows no chatter rather than an error box.
			loadError = answer.status === 404 || answer.status === 401 || answer.status === 403 ? null : answer.error;
			thread = null;
			return;
		}
		loadError = null;
		const data = answer.data;
		if (data && data.enabled) {
			thread = data;
			if (!data.can.note && mode === 'note') mode = 'message';
			if (!data.can.message && data.can.note) mode = 'note';
		} else {
			thread = null;
		}
	}

	$effect(() => {
		void plugin;
		void model;
		void recordKey;
		void showTrash;
		untrack(() => void load());
	});

	// Someone else posted: look again.
	$effect(() =>
		notificationsStore.onEvent((event) => {
			if (event.plugin !== 'chatter') return;
			const payload = event.payload as { plugin?: string; model?: string; key?: string } | null;
			if (payload?.plugin === plugin && payload?.model === model && payload?.key === recordKey) {
				void load();
			}
		})
	);

	const visible = $derived(
		(thread?.messages ?? []).filter((message) => showTrash || !message.deleted_at)
	);

	function initials(name: string): string {
		return (
			name
				.split(/\s+/)
				.filter(Boolean)
				.slice(0, 2)
				.map((part) => part[0]?.toUpperCase() ?? '')
				.join('') || '?'
		);
	}

	async function lookUp(text: string): Promise<void> {
		const match = /(?:^|\s)@([\p{L}\p{N} ._-]{0,30})$/u.exec(text);
		if (!match || !thread?.member) {
			suggestions = [];
			return;
		}
		const token = ++suggestToken;
		const answer = await chatterApi.people(match[1].trim());
		if (token === suggestToken) suggestions = answer.data?.people ?? [];
	}

	function onInput(event: Event): void {
		draft = (event.target as HTMLTextAreaElement).value;
		void lookUp(draft);
	}

	function choose(person: Person): void {
		draft = draft.replace(/@([\p{L}\p{N} ._-]{0,30})$/u, `@${person.name} `);
		picked = { ...picked, [person.name]: person.actor };
		suggestions = [];
	}

	async function post(): Promise<void> {
		if (!thread || posting || draft.trim() === '') return;
		posting = true;
		postError = null;
		const mentions = Object.entries(picked)
			.filter(([name]) => draft.includes(`@${name}`))
			.map(([, actor]) => actor);
		const answer = await chatterApi.post(plugin, model, recordKey, {
			kind: mode,
			body: draft,
			mentions,
			link,
			guest_name: thread.member ? undefined : guestName
		});
		posting = false;
		if (!answer.ok) {
			postError = answer.error;
			return;
		}
		draft = '';
		picked = {};
		suggestions = [];
		await load();
	}

	async function saveEdit(message: ChatterMessage): Promise<void> {
		const answer = await chatterApi.edit(plugin, model, recordKey, message.id, editDraft);
		if (!answer.ok) {
			failure = answer.error;
			return;
		}
		editing = null;
		failure = null;
		await load();
	}

	async function remove(message: ChatterMessage): Promise<void> {
		const answer = await chatterApi.remove(plugin, model, recordKey, message.id);
		confirmDelete = null;
		failure = answer.ok ? null : answer.error;
		await load();
	}

	async function restore(message: ChatterMessage): Promise<void> {
		const answer = await chatterApi.restore(plugin, model, recordKey, message.id);
		failure = answer.ok ? null : answer.error;
		await load();
	}

	async function follow(following: boolean, muted?: boolean): Promise<void> {
		const answer = await chatterApi.follow(plugin, model, recordKey, following, muted);
		failure = answer.ok ? null : answer.error;
		await load();
	}

	/** The body with `@Name` of the people it mentions picked out. */
	function parts(message: ChatterMessage): { text: string; mention: boolean }[] {
		const body = message.body ?? '';
		const result: { text: string; mention: boolean }[] = [];
		const pattern = /@([\p{L}\p{N}][\p{L}\p{N} ._-]*)/gu;
		let last = 0;
		for (const match of body.matchAll(pattern)) {
			const index = match.index ?? 0;
			const named = (thread?.followers ?? []).find((f) => match[0].slice(1).startsWith(f.label));
			if (!named || message.mentions.length === 0) continue;
			if (index > last) result.push({ text: body.slice(last, index), mention: false });
			result.push({ text: `@${named.label}`, mention: true });
			last = index + 1 + named.label.length;
		}
		if (last < body.length) result.push({ text: body.slice(last), mention: false });
		return result;
	}
</script>

{#if thread}
	<section
		class="mt-8 rounded-lg border bg-card text-card-foreground"
		aria-label="Chatter"
		data-testid="chatter"
	>
		<header class="flex items-center justify-between gap-3 border-b px-4 py-3">
			<h2 class="text-sm font-semibold">Chatter</h2>
			<div class="flex items-center gap-2 text-xs text-muted-foreground">
				{#if thread.can.follow}
					<span data-testid="chatter-followers">
						{thread.followers.length}
						{thread.followers.length === 1 ? 'follower' : 'followers'}
					</span>
					{#if thread.following}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => follow(true, !thread?.muted)}
							title={thread.muted ? 'Get notified again' : 'Stop notifications for this record'}
						>
							{#if thread.muted}<BellOffIcon class="size-4" /> Muted{:else}<BellIcon class="size-4" /> Notifying{/if}
						</Button>
						<Button variant="outline" size="sm" onclick={() => follow(false)}>Unfollow</Button>
					{:else}
						<Button variant="outline" size="sm" onclick={() => follow(true)}>Follow</Button>
					{/if}
				{/if}
				{#if thread.can.trash}
					<Button variant="ghost" size="sm" onclick={() => (showTrash = !showTrash)}>
						{showTrash ? 'Hide trash' : `Trash (${thread.trash_count})`}
					</Button>
				{/if}
			</div>
		</header>

		{#if thread.can.message || thread.can.note}
			<div class="space-y-2 border-b px-4 py-3">
				<div class="flex gap-1 text-sm" role="tablist">
					{#if thread.can.message}
						<button
							type="button"
							role="tab"
							aria-selected={mode === 'message'}
							class="rounded-md px-3 py-1 {mode === 'message' ? 'bg-accent font-medium text-accent-foreground' : 'text-muted-foreground hover:bg-muted'}"
							onclick={() => (mode = 'message')}>Send message</button
						>
					{/if}
					{#if thread.can.note}
						<button
							type="button"
							role="tab"
							aria-selected={mode === 'note'}
							class="rounded-md px-3 py-1 {mode === 'note' ? 'bg-accent font-medium text-accent-foreground' : 'text-muted-foreground hover:bg-muted'}"
							onclick={() => (mode = 'note')}>Log note</button
						>
					{/if}
				</div>
				{#if !thread.member}
					<input
						class="h-9 w-full rounded-md border bg-background px-3 text-sm"
						placeholder="Your name"
						maxlength="60"
						bind:value={guestName}
						aria-label="Your name"
					/>
				{/if}
				<div class="relative">
					<Textarea
						value={draft}
						oninput={onInput}
						rows={3}
						placeholder={mode === 'note'
							? 'Log an internal note. Only people who can edit this record see it.'
							: thread.member
								? 'Write a message. Followers are notified; type @ to mention someone.'
								: 'Write a message'}
						aria-label={mode === 'note' ? 'Note' : 'Message'}
					/>
					{#if suggestions.length > 0}
						<ul
							class="absolute z-10 mt-1 w-64 rounded-md border bg-popover p-1 text-sm shadow-md"
							role="listbox"
						>
							{#each suggestions as person (person.actor)}
								<li>
									<button
										type="button"
										role="option"
										aria-selected="false"
										class="w-full rounded px-2 py-1 text-left hover:bg-accent"
										onclick={() => choose(person)}>{person.name}</button
									>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
				{#if postError}<p class="text-sm text-destructive" role="alert">{postError}</p>{/if}
				<div class="flex items-center justify-between">
					<p class="text-xs text-muted-foreground">
						{#if mode === 'note'}<LockIcon class="mr-1 inline size-3" />Internal: nobody is notified unless mentioned.{:else if thread.member}Followers are notified.{/if}
					</p>
					<Button size="sm" onclick={post} disabled={posting || draft.trim() === ''}>
						{posting ? 'Posting…' : mode === 'note' ? 'Log note' : 'Send'}
					</Button>
				</div>
			</div>
		{/if}

		{#if failure}<p class="px-4 pt-3 text-sm text-destructive" role="alert">{failure}</p>{/if}

		<ol class="divide-y" data-testid="chatter-thread">
			{#each [...visible].reverse() as message (message.id)}
				<li
					class="flex gap-3 px-4 py-3 {message.deleted_at ? 'opacity-60' : ''}"
					data-kind={message.kind}
				>
					<span
						class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-full bg-primary/10 text-xs font-semibold text-primary"
						aria-hidden="true">{initials(message.author_label)}</span
					>
					<div class="min-w-0 flex-1 space-y-1">
						<div class="flex flex-wrap items-baseline gap-x-2 text-sm">
							<span class="font-medium">{message.author_label}</span>
							<span class="text-xs text-muted-foreground" title={message.created_at}>
								{timeAgo(message.created_at)}
							</span>
							{#if message.kind === 'note'}
								<span class="rounded bg-highlight/20 px-1.5 text-xs text-highlight-foreground">Note</span>
							{/if}
							{#if message.edited}<span class="text-xs text-muted-foreground">(edited)</span>{/if}
							{#if message.deleted_at}<span class="text-xs text-destructive">deleted</span>{/if}
						</div>

						{#if message.kind === 'change'}
							<ul class="space-y-0.5 text-sm text-muted-foreground">
								{#each changeLines(message, thread.tracked) as line (line.label)}
									<li>
										<span class="text-foreground">{line.label}:</span>
										{line.from} <span aria-hidden="true">→</span><span class="sr-only">to</span>
										<span class="text-foreground">{line.to}</span>
									</li>
								{/each}
							</ul>
						{:else if message.kind === 'system'}
							<p class="text-sm text-muted-foreground">{message.body}</p>
						{:else if editing === message.id}
							<Textarea bind:value={editDraft} rows={3} aria-label="Edit message" />
							<div class="flex gap-2">
								<Button size="sm" onclick={() => saveEdit(message)} disabled={editDraft.trim() === ''}>Save</Button>
								<Button size="sm" variant="ghost" onclick={() => (editing = null)}>Cancel</Button>
							</div>
						{:else}
							<p class="whitespace-pre-wrap break-words text-sm">
								{#each parts(message) as part, index (index)}{#if part.mention}<span class="rounded bg-primary/10 px-1 text-primary">{part.text}</span>{:else}{part.text}{/if}{/each}
							</p>
						{/if}

						{#if (message.kind === 'message' || message.kind === 'note') && editing !== message.id}
							<div class="flex gap-1 pt-0.5">
								{#if message.deleted_at}
									{#if thread.can.trash}
										<Button size="sm" variant="ghost" onclick={() => restore(message)}><UndoIcon class="size-3.5" /> Restore</Button>
									{/if}
								{:else if message.author === thread.me}
									<Button
										size="sm"
										variant="ghost"
										onclick={() => {
											editing = message.id;
											editDraft = message.body ?? '';
										}}><PencilIcon class="size-3.5" /> Edit</Button
									>
								{/if}
								{#if !message.deleted_at && (message.author === thread.me || thread.can.trash)}
									{#if confirmDelete === message.id}
										<Button size="sm" variant="destructive" onclick={() => remove(message)}>Move to trash</Button>
										<Button size="sm" variant="ghost" onclick={() => (confirmDelete = null)}>Keep</Button>
									{:else}
										<Button size="sm" variant="ghost" onclick={() => (confirmDelete = message.id)}><Trash2Icon class="size-3.5" /> Delete</Button>
									{/if}
								{/if}
							</div>
						{/if}
					</div>
				</li>
			{:else}
				<li class="px-4 py-6 text-center text-sm text-muted-foreground">Nothing here yet.</li>
			{/each}
		</ol>
	</section>
{:else if loadError}
	<p class="mt-8 text-sm text-destructive" role="alert">{loadError}</p>
{/if}

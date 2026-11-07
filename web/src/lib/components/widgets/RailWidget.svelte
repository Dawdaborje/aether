<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { browserStorage } from '$lib/browserStorage';
	import { navHref } from '$lib/components/layout/nav';
	import { useLiveTopic, usePresence } from '$lib/pages/liveTopic.svelte';
	import { initial, userKey } from '$lib/pages/presence';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';
	import {
		badgeText,
		filterRail,
		readRail,
		type RailFields,
		type RailGroup
	} from '$lib/pages/rail';

	/**
	 * `<rail source="my_channels" topic="inbox" active="{channel}" open="/chat/{id}"/>`: a list to move around an
	 * app by: groups (teams, folders) with items (channels, people) that show an unread badge and an online dot.
	 *
	 * `source` is called with the page's values and answers a list of rows (`id`, `name`, and optionally `group`,
	 * `unread`, `online`, `muted`) or `{ groups: [{ id, label, items: [...] }] }`. The item whose id equals
	 * `active` is marked as the current one (`default` when it is empty); `open` is where an item leads (`{id}` and other `{field}`s of the row
	 * are filled in). With `topic` the list asks `source` again whenever something is published there, so badges
	 * and dots follow what happens elsewhere. Groups fold, and the fold is remembered for the person.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const fields: RailFields = $derived({
		id: text(node.id_field) ?? 'id',
		label: [text(node.label_field) ?? 'name', 'label', 'title'],
		badge: text(node.badge_field) ?? 'unread',
		mention: text(node.mention_field) ?? 'mentions',
		online: text(node.online_field) ?? 'online',
		group: text(node.group_field) ?? 'group',
		icon: text(node.icon_field) ?? 'icon'
	});
	const searchable = $derived(node.search === true || node.search === 'true');
	const live = useLiveTopic(() => node.topic);
	// `presence="online"`: a topic everyone on the page joins; an item whose `user` is connected shows a green dot.
	const here = usePresence(() => node.presence, { keep: true });
	const online = $derived(new Set(here.users.map(userKey)));
	const idle = $derived(new Set(here.idle.map(userKey)));
	const userField = $derived(text(node.user_field) ?? 'user');
	// `avatar="true"`: a round avatar before each label: the row's `image` when it has one, else the initial of a
	// direct chat (`kind` "direct"), `+` for a group and `#` for a channel. The online dot moves onto the avatar.
	const avatars = $derived(node.avatar === true || node.avatar === 'true');

	function glyph(item: RailGroup['items'][number]): string {
		const kind = item.row.kind;
		if (kind === 'direct') return initial(item.label);
		if (kind === 'group') return '+';
		return '#';
	}

	let groups = $state<RailGroup[]>([]);
	let loaded = $state(false);
	let failure = $state<string | null>(null);
	let needle = $state('');
	let folded = $state<Set<string>>(new Set());
	let list = $state<HTMLElement | null>(null);
	let latest = 0;

	const shown = $derived(filterRail(groups, needle));
	const active = $derived.by(() => {
		const template = text(node.active);
		const value = template ? interpolate(template, page?.vars ?? {}) : '';
		// `default` is the entry marked when `active` gives none (a page opened without choosing a place).
		return value || text(node.default) || '';
	});
	const scope = $derived(`aether.rail.${page?.plugin ?? ''}.${text(node.name) ?? source ?? ''}.`);

	function readFold(): Set<string> {
		const out = new Set<string>();
		try {
			const raw = browserStorage()?.getItem(scope + 'folded');
			if (raw) for (const key of JSON.parse(raw) as string[]) out.add(String(key));
		} catch {
			/* a remembered fold is a convenience only */
		}
		return out;
	}

	function toggle(key: string) {
		const next = new Set(folded);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		folded = next;
		try {
			browserStorage()?.setItem(scope + 'folded', JSON.stringify([...next]));
		} catch {
			/* see readFold */
		}
	}

	async function load() {
		if (!source || !page) return;
		const current = ++latest;
		const result = await page.call(source, page.args);
		if (current !== latest) return;
		loaded = true;
		if (!result.ok) {
			failure = result.error;
			return;
		}
		failure = null;
		groups = readRail(result.data, fields);
	}

	// Another page, other values, a refresh of the page, or something published on the live topic.
	$effect(() => {
		void page?.plugin;
		void source;
		void JSON.stringify(page?.args ?? {});
		void page?.tick;
		void live.n;
		untrack(() => void load());
	});

	$effect(() => {
		void scope;
		untrack(() => (folded = readFold()));
	});

	function href(item: RailGroup['items'][number]): string {
		const template = text(node.open);
		return template ? navHref(interpolate(template, { ...item.row, id: item.id })) : '#';
	}

	// Arrow keys move between the entries, Home and End jump to the first and last.
	function onkeydown(event: KeyboardEvent) {
		if (!list) return;
		const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End'];
		if (!keys.includes(event.key)) return;
		const links = Array.from(list.querySelectorAll<HTMLElement>('[data-rail-item]'));
		if (links.length === 0) return;
		const at = links.indexOf(document.activeElement as HTMLElement);
		let next = at;
		if (event.key === 'ArrowDown') next = Math.min(links.length - 1, at + 1);
		else if (event.key === 'ArrowUp') next = Math.max(0, at <= 0 ? 0 : at - 1);
		else if (event.key === 'Home') next = 0;
		else next = links.length - 1;
		event.preventDefault();
		links[next]?.focus();
	}
</script>

<nav
	class="flex min-h-0 flex-1 flex-col"
	data-part="rail"
	aria-label={text(node.title) ?? text(node.label) ?? 'Navigation'}
>
	{#if searchable}
		<div class="relative shrink-0 p-2">
			<AppIcon
				name="search"
				class="pointer-events-none absolute top-1/2 left-4 size-4 -translate-y-1/2 text-muted-foreground"
			/>
			<input
				type="search"
				bind:value={needle}
				placeholder={text(node.placeholder) ?? 'Search'}
				aria-label="Filter the list"
				class="h-10 w-full rounded-md border border-input bg-transparent pr-2 pl-8 text-base outline-none focus-visible:ring-2 focus-visible:ring-ring"
			/>
		</div>
	{/if}
	<div
		bind:this={list}
		{onkeydown}
		role="presentation"
		class="min-h-0 flex-1 overflow-y-auto px-1 pb-2"
	>
		{#if failure && !loaded}
			<p class="p-3 text-sm text-destructive" role="alert">{failure}</p>
		{:else if loaded && shown.length === 0}
			<p class="p-3 text-sm text-muted-foreground">{text(node.empty) ?? 'Nothing here yet.'}</p>
		{/if}
		{#each shown as group (group.key)}
			{@const isFolded = folded.has(group.key) && !needle}
			{#if group.label}
				<button
					type="button"
					class="mt-2 flex w-full items-center gap-1 rounded px-2 py-1 text-xs font-medium tracking-wide text-muted-foreground uppercase hover:bg-muted"
					aria-expanded={!isFolded}
					onclick={() => toggle(group.key)}
				>
					{#if isFolded}<AppIcon name="chevron-right" class="size-3" />{:else}<AppIcon
							name="chevron-down"
							class="size-3"
						/>{/if}
					{group.label}
				</button>
			{/if}
			{#if !isFolded}
				<ul class="space-y-0.5">
					{#each group.items as item (item.id)}
						{@const current = active !== '' && active === item.id}
						{@const isOnline =
							item.online || (online.size > 0 && online.has(userKey(item.row[userField])))}
						{@const isIdle = isOnline && idle.has(userKey(item.row[userField]))}
						{@const dot = isIdle ? 'bg-amber-400' : 'bg-emerald-500'}
						{@const word = isIdle ? 'Idle' : 'Online'}
						<li>
							<a
								href={href(item)}
								data-rail-item
								data-active={current}
								aria-current={current ? 'page' : undefined}
								class="flex items-center gap-3 rounded-md px-2.5 py-2 text-base hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none {current
									? 'bg-accent font-medium text-accent-foreground hover:bg-accent'
									: ''} {item.muted ? 'text-muted-foreground' : ''}"
							>
								{#if avatars}
									<span
										class="relative grid size-10 shrink-0 place-items-center rounded-full bg-muted text-sm font-medium text-muted-foreground"
										data-part="rail-avatar"
										aria-hidden="true"
									>
										{#if typeof item.row.image === 'string' && item.row.image}<img
												src={item.row.image}
												alt=""
												class="size-full rounded-full object-cover"
											/>
										{:else}{glyph(item)}{/if}
										{#if isOnline}
											<span
												class="absolute right-0 bottom-0 size-3 rounded-full border-2 border-background {dot}"
												data-status={isIdle ? 'idle' : 'online'}
											></span>
										{/if}
									</span>
									{#if isOnline}<span class="sr-only">{word}</span>{/if}
								{:else if isOnline}
									<span
										class="size-2 shrink-0 rounded-full {dot}"
										title={word}
										data-status={isIdle ? 'idle' : 'online'}
									></span>
									<span class="sr-only">{word}</span>
								{/if}
								<span class="min-w-0 flex-1 truncate">{item.label}</span>
								{#if item.mentions > 0 && !current}
									<span
										class="shrink-0 rounded-full bg-destructive px-1.5 text-[11px] leading-5 font-medium text-white"
										aria-label="{item.mentions} unread mention{item.mentions === 1 ? '' : 's'}"
										data-part="rail-mention">@{item.mentions}</span
									>
								{:else if item.badge > 0 && !current}
									<span
										class="shrink-0 rounded-full bg-primary px-1.5 text-[11px] leading-5 font-medium text-primary-foreground"
										aria-label="{item.badge} unread"
										data-part="rail-badge">{badgeText(item.badge)}</span
									>
								{/if}
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		{/each}
	</div>
</nav>

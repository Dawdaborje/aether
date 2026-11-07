<script lang="ts">
	import { browserStorage } from '$lib/browserStorage';
	import { inGroup, searchPack, toned, type EmojiPack, type PackEmoji } from '$lib/pages/emoji';

	/**
	 * The panel of the composer's emoji button: tabs for the groups of the pack, a search box, a skin tone for people
	 * and hands, and the emoji used last. `onpick` gets the character to put into the message.
	 */
	let {
		pack,
		onpick,
		onclose
	}: { pack: EmojiPack; onpick: (emoji: string) => void; onclose: () => void } = $props();

	const RECENT = 'aether.emoji.recent';
	const TONE = 'aether.emoji.tone';
	const TONES = ['👋', '👋🏻', '👋🏼', '👋🏽', '👋🏾', '👋🏿'];

	function read<T>(key: string, fallback: T): T {
		try {
			const raw = browserStorage()?.getItem(key);
			return raw ? (JSON.parse(raw) as T) : fallback;
		} catch {
			return fallback; // a remembered choice is a convenience only
		}
	}

	function write(key: string, value: unknown) {
		try {
			browserStorage()?.setItem(key, JSON.stringify(value));
		} catch {
			/* see read */
		}
	}

	let query = $state('');
	let group = $state(0);
	let tone = $state(Math.min(5, Math.max(0, Number(read<number>(TONE, 0)) || 0)));
	let recent = $state<string[]>(read<string[]>(RECENT, []).filter((c) => typeof c === 'string'));

	const searching = $derived(query.trim() !== '');
	const shown = $derived(
		searching ? searchPack(pack, query) : group < 0 ? [] : inGroup(pack, group)
	);
	const recentEmoji = $derived(
		recent
			.map((c) => pack.emoji.find((e) => e.c === c || e.k?.includes(c)))
			.filter((e): e is PackEmoji => !!e)
	);

	function choose(emoji: PackEmoji) {
		const c = toned(emoji, tone);
		recent = [c, ...recent.filter((r) => r !== c)].slice(0, 16);
		write(RECENT, recent);
		onpick(c);
	}

	function pickTone(next: number) {
		tone = next;
		write(TONE, next);
	}
</script>

<div
	class="absolute bottom-full left-0 z-20 mb-1 w-96 max-w-[calc(100vw-1rem)] rounded-md border border-border bg-background p-2 shadow"
	role="dialog"
	aria-label="Emoji"
	data-part="emoji-picker"
	tabindex="-1"
	onkeydown={(e) => e.key === 'Escape' && onclose()}
>
	<div class="mb-2 flex items-center gap-2">
		<!-- svelte-ignore a11y_autofocus -->
		<input
			type="search"
			bind:value={query}
			autofocus
			placeholder="Search emoji"
			aria-label="Search emoji"
			class="h-8 min-w-0 flex-1 rounded-md border border-input bg-transparent px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
		/>
		<div class="flex shrink-0 gap-0.5" role="group" aria-label="Skin tone">
			{#each TONES as sample, i (i)}
				<button
					type="button"
					class="grid size-6 place-items-center rounded text-sm {tone === i
						? 'bg-secondary'
						: 'hover:bg-muted'}"
					aria-label={i === 0 ? 'Default skin tone' : `Skin tone ${i}`}
					aria-pressed={tone === i}
					onmousedown={(e) => e.preventDefault()}
					onclick={() => pickTone(i)}>{sample}</button
				>
			{/each}
		</div>
	</div>
	{#if !searching}
		<div class="mb-1 flex gap-0.5 border-b border-border pb-1" role="tablist" aria-label="Groups">
			{#if recentEmoji.length > 0}
				<button
					type="button"
					role="tab"
					class="grid size-7 place-items-center rounded text-base {group < 0
						? 'bg-secondary'
						: 'hover:bg-muted'}"
					aria-selected={group < 0}
					title="Recent"
					onmousedown={(e) => e.preventDefault()}
					onclick={() => (group = -1)}>🕘</button
				>
			{/if}
			{#each pack.groups as g, i (g.key)}
				<button
					type="button"
					role="tab"
					class="grid size-7 place-items-center rounded text-base {group === i
						? 'bg-secondary'
						: 'hover:bg-muted'}"
					aria-selected={group === i}
					title={g.name}
					onmousedown={(e) => e.preventDefault()}
					onclick={() => (group = i)}>{inGroup(pack, i)[0]?.c ?? '•'}</button
				>
			{/each}
		</div>
	{/if}
	<div class="h-60 overflow-y-auto" role="tabpanel">
		{#if !searching && group < 0}
			<div class="grid grid-cols-8 gap-0.5">
				{#each recentEmoji as emoji (emoji.c)}
					{@render cell(emoji)}
				{/each}
			</div>
		{:else if shown.length === 0}
			<p class="p-2 text-sm text-muted-foreground">No emoji found.</p>
		{:else}
			<p class="px-1 pb-0.5 text-xs font-medium text-muted-foreground capitalize">
				{searching ? 'Results' : pack.groups[group]?.name}
			</p>
			<div class="grid grid-cols-8 gap-0.5">
				{#each shown as emoji (emoji.c)}
					{@render cell(emoji)}
				{/each}
			</div>
		{/if}
	</div>
</div>

{#snippet cell(emoji: PackEmoji)}
	<button
		type="button"
		class="grid size-10 place-items-center rounded text-xl hover:bg-muted"
		title={emoji.n}
		aria-label={emoji.n}
		onmousedown={(e) => e.preventDefault()}
		onclick={() => choose(emoji)}>{toned(emoji, tone)}</button
	>
{/snippet}

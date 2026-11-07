<script lang="ts">
	import * as Command from '$lib/components/ui/command/index.js';
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { groupByKind, screenSearch } from './screenSearch.svelte';
	import { formatShortcut, shortcutOf } from './shortcuts';

	let query = $state('');
	const groups = $derived(groupByKind(screenSearch.find(query)));

	// Start from a clean box each time it opens.
	$effect(() => {
		if (screenSearch.open) query = '';
	});

	// Ask the plugins' record searches once typing pauses.
	$effect(() => {
		const typed = query;
		if (!screenSearch.open) return;
		const timer = setTimeout(() => void screenSearch.findRecords(typed), 200);
		return () => clearTimeout(timer);
	});

	function onKeydown(event: KeyboardEvent) {
		// Only signed-in people have screens to find.
		if (!authSession.isUser || event.defaultPrevented) return;
		if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === 'k') {
			event.preventDefault();
			screenSearch.toggle();
			return;
		}
		// A plugin's palette entry with this shortcut (`[[app.palette]] shortcut`).
		const pressed = shortcutOf(event);
		const entry = pressed && screenSearch.index.find((item) => item.shortcut === pressed);
		if (entry) {
			event.preventDefault();
			void screenSearch.go(entry);
		}
	}
</script>

<svelte:window onkeydown={onKeydown} />

<!-- The ranking is ours (accents, words, typos), so the primitive's own filter is off. -->
<Command.Dialog
	bind:open={screenSearch.open}
	shouldFilter={false}
	title="Find a screen or command"
	description="Type to find a screen, then press Enter to open it."
>
	<Command.Input bind:value={query} placeholder="Find a screen or command…" />
	<Command.List>
		{#if screenSearch.loading}
			<Command.Loading>Loading screens…</Command.Loading>
		{/if}
		<Command.Empty>No screen matches “{query}”.</Command.Empty>
		{#each groups as group (group.heading)}
			<Command.Group heading={group.heading}>
				{#each group.entries as entry (entry.id)}
					{@const Icon = iconRef(entry.icon)}
					<Command.Item value={entry.id} onSelect={() => void screenSearch.go(entry)}>
						{#if Icon}<AppIcon name={Icon} class="text-muted-foreground" />{/if}
						<span class="truncate">{entry.label}</span>
						{#if entry.context}
							<span class="truncate text-xs text-muted-foreground">{entry.context}</span>
						{/if}
						{#if entry.shortcut}
							<Command.Shortcut>{formatShortcut(entry.shortcut)}</Command.Shortcut>
						{/if}
					</Command.Item>
				{/each}
			</Command.Group>
		{/each}
		{#if screenSearch.records.length > 0}
			<Command.Group heading="Records">
				{#each screenSearch.records as hit (hit.id)}
					<Command.Item value={hit.id} onSelect={() => void screenSearch.go(hit)}>
						<AppIcon name="search" class="text-muted-foreground" />
						<span class="truncate">{hit.label}</span>
						<span class="truncate text-xs text-muted-foreground">{hit.context}</span>
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}
	</Command.List>
</Command.Dialog>

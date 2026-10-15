<script lang="ts">
	import StarIcon from '@lucide/svelte/icons/star';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Popover from '$lib/components/ui/popover';
	import type { PageChrome } from '$lib/pages/pageChrome.svelte';
	import {
		createSearchFavorite,
		deleteSearchFavorite,
		fetchSearchFavorites,
		type SearchFavorite
	} from './favoritesApi';
	import { chipsToFilter, chipsToQ } from './filterCodec';
	import { chipsFromFilterJson } from './urlState';

	let { chrome }: { chrome: PageChrome } = $props();

	let open = $state(false);
	let favorites = $state<SearchFavorite[]>([]);
	let loading = $state(false);
	let saving = $state(false);
	let label = $state('');
	let error = $state<string | null>(null);

	async function reload() {
		if (!chrome.model) {
			favorites = [];
			return;
		}
		loading = true;
		error = null;
		try {
			favorites = await fetchSearchFavorites(chrome.model);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not load favorites.';
			favorites = [];
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (open) void reload();
	});

	async function save() {
		const name = label.trim();
		if (!chrome.model || !name || saving) return;
		saving = true;
		error = null;
		try {
			await createSearchFavorite({
				model: chrome.model,
				label: name,
				filter: chipsToFilter(chrome.chips),
				q: chipsToQ(chrome.chips) ?? null,
				group_by: chrome.groupBy
			});
			label = '';
			await reload();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not save.';
		} finally {
			saving = false;
		}
	}

	function apply(fav: SearchFavorite) {
		chrome.setChips(chipsFromFilterJson(fav.filter, fav.q));
		chrome.setGroupBy(fav.group_by ?? []);
		open = false;
	}

	async function remove(fav: SearchFavorite) {
		error = null;
		try {
			await deleteSearchFavorite(fav.id);
			await reload();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not delete.';
		}
	}
</script>

{#if chrome.model}
	<Popover.Root bind:open>
		<Popover.Trigger>
			{#snippet child({ props })}
				<Button variant="outline" size="sm" {...props}>
					<StarIcon data-icon="inline-start" />
					Favorites
				</Button>
			{/snippet}
		</Popover.Trigger>
		<Popover.Content class="w-72 space-y-3 p-3" align="end">
			<p class="text-xs font-medium text-muted-foreground">Favorites</p>
			{#if loading}
				<p class="text-xs text-muted-foreground">Loading…</p>
			{:else if favorites.length === 0}
				<p class="text-xs text-muted-foreground">No saved searches yet.</p>
			{:else}
				<ul class="max-h-48 space-y-1 overflow-y-auto">
					{#each favorites as fav (fav.id)}
						<li class="flex items-center gap-1">
							<button
								type="button"
								class="min-w-0 flex-1 truncate rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted"
								onclick={() => apply(fav)}
							>
								{fav.label}
							</button>
							<Button
								variant="ghost"
								size="icon-xs"
								aria-label={`Delete ${fav.label}`}
								onclick={() => remove(fav)}
							>
								×
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="space-y-2 border-t border-border pt-2">
				<p class="text-xs text-muted-foreground">Save current search</p>
				<div class="flex gap-2">
					<Input
						bind:value={label}
						placeholder="Name"
						class="h-8"
						onkeydown={(event) => {
							if (event.key === 'Enter') {
								event.preventDefault();
								void save();
							}
						}}
					/>
					<Button size="sm" disabled={saving || !label.trim()} onclick={() => void save()}>
						Save
					</Button>
				</div>
			</div>
			{#if error}
				<p class="text-xs text-destructive" role="alert">{error}</p>
			{/if}
		</Popover.Content>
	</Popover.Root>
{/if}

<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import AppTile from '$lib/components/apps/AppTile.svelte';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';
	import { appsStore } from '$lib/apps/appsStore.svelte';

	/**
	 * `<tiles title="Communicate" cols="4"><tile label="Chat" description="…" href="/chat" plugin="atrium_chat"
	 * icon="messages-square"/></tiles>`: a grid of places to go, drawn like the tiles of the Apps launcher, for a
	 * plugin's home page. A tile shows the logo of `plugin` when that plugin ships one, else the `icon` (a name from
	 * the launcher's icon set), else the first letter of its label. `requires_route="/path"` shows the tile only
	 * while some app menu the person can see links to that page, so a home page can offer what an optional add-on
	 * provides without a dead link (an add-on folded into this app counts as soon as its menu is listed).
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const reachable = $derived(
		new Set(
			(appsStore.apps ?? []).flatMap((app) => [app.route, ...(app.menus ?? []).map((m) => m.href)])
		)
	);
	const tiles = $derived(
		(node.children ?? [])
			.filter((child) => child.type === 'tile')
			.filter((child) => {
				const needed = text(child.requires_route);
				return !needed || reachable.has(needed);
			})
	);
	const cols = $derived.by(() => {
		const n = Number(node.cols);
		return n >= 1 && n <= 6 ? Math.floor(n) : 4;
	});
	const columns = $derived(
		(
			{
				1: 'grid-cols-1',
				2: 'grid-cols-2',
				3: 'grid-cols-2 sm:grid-cols-3',
				4: 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-4',
				5: 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-5',
				6: 'grid-cols-2 sm:grid-cols-3 lg:grid-cols-6'
			} as Record<number, string>
		)[cols]
	);
</script>

<section class="px-3 py-4" data-part="tiles">
	{#if node.title}
		<h3 class="mb-2 px-1 text-sm font-semibold text-foreground">{text(node.title)}</h3>
	{/if}
	<ul class="grid gap-1 {columns}">
		{#each tiles as tile, i (tile.name ?? `${text(tile.label)}-${i}`)}
			{@const plugin = text(tile.plugin) ?? ''}
			<li>
				<AppTile
					app={{
						plugin,
						label: text(tile.label) ?? '',
						icon: text(tile.icon) ?? null,
						logo_url: plugin ? `/api/ui/plugins/${plugin}/logo` : null,
						route: interpolate(text(tile.href) ?? '#', page?.vars ?? {}),
						description: text(tile.description) ?? null
					}}
				/>
			</li>
		{/each}
	</ul>
</section>

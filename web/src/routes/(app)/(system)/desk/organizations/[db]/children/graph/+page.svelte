<script lang="ts">
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { getContext } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Empty } from '$lib/components/ui/empty';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { ancestorsOf, descendantsOf, type OrgNode } from '$lib/org/hierarchy';
	import type { OrganizationProfile } from '$lib/org/api';

	let { params }: { params: { db: string } } = $props();
	const ctx = getContext<{ organizations: OrganizationProfile[] | null; failed: string | null }>(
		'org-children'
	);

	const W = 160;
	const H = 40;
	const GAP_X = 24;
	const GAP_Y = 56;

	interface Placed {
		org: OrganizationProfile;
		x: number;
		y: number;
		parent: Placed | null;
	}

	/** Tidy tree: leaves take consecutive columns, a parent centres over its children. */
	const layout = $derived.by(() => {
		if (!ctx.organizations) return null;
		const all = ctx.organizations;
		const self = all.find((org) => org.db_name === params.db);
		if (!self) return null;
		const above = ancestorsOf(all, params.db).reverse();
		const tree = descendantsOf(all, params.db);
		const placed: Placed[] = [];
		let column = 0;
		const place = (node: OrgNode, depth: number, parent: Placed | null): Placed => {
			const me: Placed = { org: node.org, x: 0, y: (above.length + depth) * (H + GAP_Y), parent };
			const kids = node.children.map((child) => place(child, depth + 1, me));
			me.x = kids.length ? (kids[0].x + kids[kids.length - 1].x) / 2 : column++ * (W + GAP_X);
			placed.push(me);
			return me;
		};
		const root: Placed = { org: self, x: 0, y: above.length * (H + GAP_Y), parent: null };
		const kids = tree.map((node) => place(node, 1, root));
		root.x = kids.length ? (kids[0].x + kids[kids.length - 1].x) / 2 : 0;
		placed.push(root);
		// Ancestors stack straight above the organization.
		let previous: Placed | null = root;
		above
			.slice()
			.reverse()
			.forEach((org, index) => {
				const node: Placed = {
					org,
					x: root.x,
					y: (above.length - 1 - index) * (H + GAP_Y),
					parent: null
				};
				if (previous) previous.parent = node;
				previous = node;
				placed.push(node);
			});
		const width = Math.max(column, 1) * (W + GAP_X);
		const height = (above.length + 1) * (H + GAP_Y) + 0;
		return { placed, width, height, rootDb: params.db, hasKids: kids.length > 0 };
	});

	function label(text: string) {
		return text.length > 22 ? `${text.slice(0, 21)}…` : text;
	}
</script>

{#if ctx.organizations === null}
	<Skeleton class="h-48 w-full rounded-lg" />
{:else if ctx.failed}
	<LoadFailed message={ctx.failed} />
{:else if !layout || (!layout.hasKids && layout.placed.length === 1)}
	<Empty
		icon="network"
		title="Nothing connected"
		description="This organization has no parent and no children."
	/>
{:else}
	{@const pad = 16}
	{@const minX = Math.min(...layout.placed.map((p) => p.x))}
	{@const maxX = Math.max(...layout.placed.map((p) => p.x)) + W}
	{@const maxY = Math.max(...layout.placed.map((p) => p.y)) + H}
	<div class="min-h-64 flex-1 overflow-auto rounded-lg border border-border bg-card p-2">
		<svg
			width={maxX - minX + pad * 2}
			height={maxY + pad * 2}
			viewBox="{minX - pad} {-pad} {maxX - minX + pad * 2} {maxY + pad * 2}"
			role="img"
			aria-label="Organization hierarchy"
		>
			{#each layout.placed as node (node.org.db_name)}
				{#if node.parent}
					<path
						d="M {node.parent.x + W / 2} {node.parent.y + H} C {node.parent.x + W / 2} {node.parent
							.y +
							H +
							GAP_Y / 2}, {node.x + W / 2} {node.y - GAP_Y / 2}, {node.x + W / 2} {node.y}"
						fill="none"
						stroke="currentColor"
						class="text-border"
						stroke-width="1.5"
					/>
				{/if}
			{/each}
			{#each layout.placed as node (node.org.db_name)}
				{@const current = node.org.db_name === layout.rootDb}
				<g
					role="link"
					tabindex="0"
					class="cursor-pointer"
					onclick={() => goto(resolve(`/desk/organizations/${node.org.db_name}/children`))}
					onkeydown={(event) =>
						event.key === 'Enter' &&
						goto(resolve(`/desk/organizations/${node.org.db_name}/children`))}
				>
					<title>{node.org.name} ({node.org.db_name})</title>
					<rect
						x={node.x}
						y={node.y}
						width={W}
						height={H}
						rx="8"
						class={current ? 'fill-primary stroke-primary' : 'fill-muted stroke-border'}
					/>
					<text
						x={node.x + W / 2}
						y={node.y + H / 2 + 4}
						text-anchor="middle"
						font-size="12"
						class={current ? 'fill-primary-foreground' : 'fill-foreground'}
						>{label(node.org.name)}</text
					>
				</g>
			{/each}
		</svg>
	</div>
	<p class="shrink-0 text-xs text-muted-foreground">
		Parents above, children below. Select an organization to centre the graph on it.
	</p>
{/if}

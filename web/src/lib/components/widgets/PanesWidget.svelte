<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import { goto } from '$app/navigation';
	import type { PageNode } from '$lib/dsl/types';
	import { browserStorage } from '$lib/browserStorage';
	import { navHref } from '$lib/components/layout/nav';
	import { Button } from '$lib/components/ui/button';
	import { getPageState } from '$lib/pages/pageContext.svelte';
	import { clampWidth, narrowPane, pixels, storageKey, visiblePanes } from '$lib/pages/panes';
	import PageRenderer from './PageRenderer.svelte';

	/**
	 * `<panes>`: regions side by side. On a wide screen they sit in a row (or a column with `direction="column"`),
	 * a pane with a `width` can be dragged between its `min` and `max`, and a `collapsible` one folds to a strip. The
	 * size and fold are remembered per person. On a narrow screen one pane shows at a time, with a Back button.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const wide = new MediaQuery('min-width: 768px', true);
	const vars = $derived(page?.vars ?? {});
	const visible = $derived(visiblePanes(node, vars));
	const column = $derived(String(node.direction ?? 'row') === 'column');
	const route = $derived(String(page?.plugin ?? '') + String(node.name ?? ''));
	const shown = $derived(narrowPane(node, visible, vars));

	let widths = $state<Record<string, number>>({});
	let folded = $state<Record<string, boolean>>({});

	function nameOf(pane: PageNode, index: number): string {
		return String(pane.name ?? `pane${index}`);
	}

	function read(key: string): string | null {
		try {
			return browserStorage()?.getItem(key) ?? null;
		} catch {
			return null;
		}
	}
	function write(key: string, value: string | null): void {
		try {
			if (value === null) browserStorage()?.removeItem(key);
			else browserStorage()?.setItem(key, value);
		} catch {
			/* a remembered size is a convenience only */
		}
	}

	// Remembered sizes come back once the panes are known.
	$effect(() => {
		const nextWidths: Record<string, number> = {};
		const nextFolded: Record<string, boolean> = {};
		for (const [i, pane] of visible.entries()) {
			const name = nameOf(pane, i);
			const saved = Number(read(storageKey(route, name, 'width')));
			const base = pixels(pane.width);
			if (base) {
				const min = pixels(pane.min) ?? 120;
				const max = pixels(pane.max) ?? 800;
				nextWidths[name] = clampWidth(saved > 0 ? saved : base, min, max);
			}
			nextFolded[name] = read(storageKey(route, name, 'collapsed')) === '1';
		}
		widths = nextWidths;
		folded = nextFolded;
	});

	function resize(pane: PageNode, name: string, to: number): void {
		const min = pixels(pane.min) ?? 120;
		const max = pixels(pane.max) ?? 800;
		const next = clampWidth(to, min, max);
		widths[name] = next;
		write(storageKey(route, name, 'width'), String(next));
	}

	function drag(event: PointerEvent, pane: PageNode, name: string, sign: 1 | -1): void {
		event.preventDefault();
		const startX = event.clientX;
		const startY = event.clientY;
		const start = widths[name] ?? pixels(pane.width) ?? 240;
		const move = (e: PointerEvent) => {
			const delta = column ? e.clientY - startY : e.clientX - startX;
			resize(pane, name, start + sign * delta);
		};
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}

	function key(event: KeyboardEvent, pane: PageNode, name: string, sign: 1 | -1): void {
		const forward = column ? 'ArrowDown' : 'ArrowRight';
		const back = column ? 'ArrowUp' : 'ArrowLeft';
		if (event.key !== forward && event.key !== back) return;
		event.preventDefault();
		const step = event.key === forward ? 16 : -16;
		resize(pane, name, (widths[name] ?? pixels(pane.width) ?? 240) + sign * step);
	}

	function fold(name: string): void {
		folded[name] = !folded[name];
		write(storageKey(route, name, 'collapsed'), folded[name] ? '1' : null);
	}

	function goBack(): void {
		const back = typeof node.back === 'string' ? node.back : '';
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref applies base
		if (back) void goto(navHref(back));
		else history.back();
	}
</script>

{#if wide.current}
	<div
		data-part="panes"
		class="flex min-h-0 w-full flex-1 overflow-hidden {column ? 'flex-col' : 'flex-row'}"
	>
		{#each visible as pane, i (nameOf(pane, i))}
			{@const name = nameOf(pane, i)}
			{@const sized = pixels(pane.width) !== undefined}
			{@const isFolded = folded[name] && pane.collapsible === true}
			<section
				data-part="pane"
				data-pane={name}
				class="relative flex min-h-0 min-w-0 flex-col overflow-hidden {sized || isFolded
					? 'shrink-0'
					: 'flex-1'} {i > 0 ? (column ? 'border-t' : 'border-l') : ''} border-border"
				style={isFolded
					? column
						? 'height:2.5rem'
						: 'width:2.5rem'
					: sized
						? column
							? `height:${widths[name] ?? pixels(pane.width)}px`
							: `width:${widths[name] ?? pixels(pane.width)}px`
						: undefined}
			>
				{#if pane.collapsible === true}
					<Button
						size="icon-xs"
						variant="ghost"
						class="absolute top-1 right-1 z-10"
						aria-label={isFolded ? `Show ${pane.title ?? name}` : `Hide ${pane.title ?? name}`}
						aria-expanded={!isFolded}
						onclick={() => fold(name)}
					>
						{#if isFolded}<AppIcon name="chevron-right" />{:else}<AppIcon
								name="chevron-left"
							/>{/if}
					</Button>
				{/if}
				{#if !isFolded}
					<div
						class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain {pane.collapsible ===
						true
							? 'pr-9'
							: ''}"
					>
						{#each pane.children ?? [] as child, k (child.name ?? `${child.type}-${k}`)}
							<PageRenderer node={child} />
						{/each}
					</div>
				{/if}
				{#if sized && !isFolded && i < visible.length - 1}
					<!-- The edge that is dragged: it belongs to this pane and resizes it. -->
					<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -- a window splitter is focusable and moved with the arrow keys -->
					<div
						role="separator"
						tabindex="0"
						aria-orientation={column ? 'horizontal' : 'vertical'}
						aria-label="Resize {pane.title ?? name}"
						aria-valuenow={widths[name] ?? pixels(pane.width)}
						class="absolute z-10 {column
							? 'right-0 bottom-0 left-0 h-1 cursor-row-resize'
							: 'top-0 right-0 bottom-0 w-1 cursor-col-resize'} hover:bg-primary/40 focus-visible:bg-primary/60"
						onpointerdown={(e) => drag(e, pane, name, 1)}
						onkeydown={(e) => key(e, pane, name, 1)}
					></div>
				{/if}
			</section>
		{/each}
	</div>
{:else if shown >= 0}
	{@const pane = visible[shown]}
	<div data-part="panes" class="flex min-h-0 w-full flex-1 flex-col overflow-hidden">
		{#if shown > 0}
			<div class="shrink-0 border-b border-border px-2 py-1">
				<Button size="sm" variant="ghost" onclick={goBack}
					><AppIcon name="arrow-left" /> Back</Button
				>
			</div>
		{/if}
		<section
			data-part="pane"
			data-pane={nameOf(pane, shown)}
			class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain"
		>
			{#each pane.children ?? [] as child, k (child.name ?? `${child.type}-${k}`)}
				<PageRenderer node={child} />
			{/each}
		</section>
	</div>
{/if}

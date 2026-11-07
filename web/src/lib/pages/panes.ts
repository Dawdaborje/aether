import { showWhen } from './pageContext.svelte';
import type { PageNode } from '$lib/dsl/types';

/** The `<pane>` children of a `<panes>` that are shown now (their `show` condition holds). */
export function visiblePanes(node: PageNode, vars: Record<string, string>): PageNode[] {
	return (node.children ?? []).filter(
		(child) => child.type === 'pane' && showWhen(child.show, vars)
	);
}

/**
 * Which pane a narrow screen shows. A phone shows one pane at a time: the last visible one (the most detailed),
 * unless `focus` says there is nothing to look at yet (`focus="{channel}"` while no channel is chosen), in which
 * case it is the first (the list to choose from).
 */
export function narrowPane(
	node: PageNode,
	visible: PageNode[],
	vars: Record<string, string>
): number {
	if (visible.length === 0) return -1;
	if (node.focus !== undefined && !showWhen(node.focus, vars)) return 0;
	return visible.length - 1;
}

export function clampWidth(width: number, min: number, max: number): number {
	return Math.round(Math.min(Math.max(width, min), max));
}

/** A number attribute (`width="260"`), or `undefined` when it is not a positive number. */
export function pixels(value: unknown): number | undefined {
	const n = typeof value === 'number' ? value : Number(value);
	return Number.isFinite(n) && n > 0 ? n : undefined;
}

export function storageKey(route: string, pane: string, what: 'width' | 'collapsed'): string {
	return `aether.panes.${route}.${pane}.${what}`;
}

import type { CatalogPlugin } from './catalogApi';
import { groupCatalogByName } from './catalogGroup';

export interface GraphNode {
	name: string;
	label: string;
	/** Negative: a dependency (how many hops away). Positive: a dependent. 0: the plugin. */
	level: number;
	/** Not in the catalog, so its own dependencies are unknown. */
	missing: boolean;
	x: number;
	y: number;
}

export interface GraphEdge {
	/** The plugin that lists `to` under `dependencies`. */
	from: string;
	to: string;
}

export interface DependencyGraph {
	nodes: GraphNode[];
	edges: GraphEdge[];
	width: number;
	height: number;
	dependencyCount: number;
	dependentCount: number;
}

export const NODE_WIDTH = 150;
export const NODE_HEIGHT = 30;
const COLUMN_GAP = 64;
const ROW_GAP = 12;
const PAD = 8;

/** Walk `next` from `start`, returning each reached name with its hop count (shortest path). */
function walk(start: string, next: (name: string) => string[]): Map<string, number> {
	const distance = new Map<string, number>([[start, 0]]);
	let frontier = [start];
	while (frontier.length > 0) {
		const following: string[] = [];
		for (const name of frontier) {
			for (const neighbour of next(name)) {
				if (distance.has(neighbour)) continue;
				distance.set(neighbour, distance.get(name)! + 1);
				following.push(neighbour);
			}
		}
		frontier = following;
	}
	distance.delete(start);
	return distance;
}

/**
 * The plugins `root` needs (transitively) and the plugins that need it, laid out in columns:
 * the deepest dependencies on the left, `root` in the middle, dependents on the right.
 */
export function buildDependencyGraph(root: string, catalog: CatalogPlugin[]): DependencyGraph {
	const latest = new Map(groupCatalogByName(catalog).map((g) => [g.name, g.latest]));
	const dependenciesOf = (name: string) => latest.get(name)?.dependencies ?? [];
	const dependents = new Map<string, string[]>();
	for (const [name, plugin] of latest) {
		for (const dep of plugin.dependencies) {
			const list = dependents.get(dep) ?? [];
			list.push(name);
			dependents.set(dep, list);
		}
	}

	const down = walk(root, dependenciesOf);
	const up = walk(root, (name) => dependents.get(name) ?? []);

	const levels = new Map<string, number>([[root, 0]]);
	for (const [name, hops] of down) levels.set(name, -hops);
	// A plugin on both sides would be a cycle; keep it where it was first found.
	for (const [name, hops] of up) if (!levels.has(name)) levels.set(name, hops);

	const edges: GraphEdge[] = [];
	for (const name of levels.keys()) {
		for (const dep of dependenciesOf(name)) {
			if (levels.has(dep)) edges.push({ from: name, to: dep });
		}
	}

	const columns = new Map<number, string[]>();
	for (const [name, level] of levels) {
		const list = columns.get(level) ?? [];
		list.push(name);
		columns.set(level, list);
	}
	const label = (name: string) => latest.get(name)?.label ?? name;
	const sortedLevels = [...columns.keys()].sort((a, b) => a - b);
	const tallest = Math.max(...[...columns.values()].map((c) => c.length));
	const height = PAD * 2 + tallest * NODE_HEIGHT + (tallest - 1) * ROW_GAP;

	const nodes: GraphNode[] = [];
	sortedLevels.forEach((level, column) => {
		const names = columns.get(level)!.sort((a, b) => label(a).localeCompare(label(b)));
		const used = names.length * NODE_HEIGHT + (names.length - 1) * ROW_GAP;
		const top = PAD + (height - PAD * 2 - used) / 2;
		names.forEach((name, row) => {
			nodes.push({
				name,
				label: label(name),
				level,
				missing: !latest.has(name),
				x: PAD + column * (NODE_WIDTH + COLUMN_GAP),
				y: top + row * (NODE_HEIGHT + ROW_GAP)
			});
		});
	});

	return {
		nodes,
		edges,
		width: PAD * 2 + sortedLevels.length * NODE_WIDTH + (sortedLevels.length - 1) * COLUMN_GAP,
		height,
		dependencyCount: down.size,
		dependentCount: up.size
	};
}

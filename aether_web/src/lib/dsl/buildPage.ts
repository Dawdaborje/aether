import type { BuildPageResult, PageDef, PageNode } from './types';
import { xmlTagMap } from './xmlHints';

const knownTypes = new Set<string>(Object.values(xmlTagMap));

function asRecord(value: unknown): Record<string, unknown> | null {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: null;
}

function normalizeNode(raw: unknown, warnings: string[], path: string): PageNode | null {
	const obj = asRecord(raw);
	if (!obj) {
		warnings.push(`${path}: expected object node`);
		return null;
	}

	const type = typeof obj.type === 'string' ? obj.type : null;
	if (!type) {
		warnings.push(`${path}: missing type`);
		return null;
	}

	if (!knownTypes.has(type)) {
		warnings.push(`${path}: unknown type "${type}"`);
	}

	const childrenRaw = obj.children;
	let children: PageNode[] | undefined;
	if (Array.isArray(childrenRaw)) {
		children = childrenRaw
			.map((child, i) => normalizeNode(child, warnings, `${path}/${type}[${i}]`))
			.filter((n): n is PageNode => n !== null);
	}

	return {
		...obj,
		type,
		children
	};
}

/**
 * Build a normalized page tree from API / compiled XML JSON.
 * Pure TypeScript — Svelte only renders the result.
 */
export function buildPage(input: unknown): BuildPageResult {
	const warnings: string[] = [];
	const root = normalizeNode(input, warnings, 'page');

	if (!root || root.type !== 'page') {
		warnings.push('root must be type "page"');
		return {
			page: {
				type: 'page',
				title: 'Untitled',
				children: root ? [root] : []
			},
			warnings
		};
	}

	const page: PageDef = {
		...root,
		type: 'page',
		title: typeof root.title === 'string' ? root.title : 'Untitled',
		route: typeof root.route === 'string' ? root.route : undefined,
		children: root.children ?? []
	};

	return { page, warnings };
}

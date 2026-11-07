import type { BuildPageResult, PageDef, PageNode } from './types';
import { xmlTagMap } from './xmlHints';

const knownTypes = new Set<string>([
	...Object.values(xmlTagMap),
	// View types often appear as element types after compile or authoring shortcuts
	'list',
	'form',
	'kanban',
	'dashboard',
	'calendar'
]);

function asRecord(value: unknown): Record<string, unknown> | null {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: null;
}

function viewTypeOf(node: PageNode): string | undefined {
	if (typeof node.viewType === 'string') return node.viewType;
	if (typeof node.view_type === 'string') return node.view_type;
	if (
		node.type === 'kanban' ||
		node.type === 'dashboard' ||
		node.type === 'list' ||
		node.type === 'form'
	) {
		return node.type;
	}
	return undefined;
}

/**
 * Flatten `<templates><card/><menu/></templates>` into top-level
 * `card` + `actions` so KanbanViewWidget has one shape to read.
 */
function normalizeKanbanChildren(children: PageNode[]): PageNode[] {
	const templates = children.find((c) => c.type === 'templates');
	if (!templates) {
		// menu at top level → actions
		return children.map((c) => (c.type === 'menu' ? { ...c, type: 'actions' } : c));
	}

	const rest = children.filter((c) => c.type !== 'templates');
	const fromTemplates = templates.children ?? [];
	const card = fromTemplates.find((c) => c.type === 'card');
	const menu = fromTemplates.find((c) => c.type === 'menu' || c.type === 'actions');

	const out = [...rest];
	if (card && !out.some((c) => c.type === 'card')) out.push(card);
	if (menu && !out.some((c) => c.type === 'actions' || c.type === 'menu')) {
		out.push({ ...menu, type: 'actions' });
	}
	return out;
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

	let node: PageNode = {
		...obj,
		type,
		children
	};

	const vt = viewTypeOf(node);
	if ((type === 'view' && vt === 'kanban') || type === 'kanban') {
		node = { ...node, children: normalizeKanbanChildren(node.children ?? []) };
	}

	return node;
}

const FORM_BAR = new Set(['buttonbox', 'header', 'statusbar', 'button', 'action', 'actions']);

/**
 * A page with one form view that has buttons or a state tracker beside it (`<buttonbox>`,
 * `<header>`, `<statusbar>` as siblings of the view) gets them moved into the form, so they
 * sit in the form's sticky top bar with Save instead of scrolling away above it.
 */
function hoistFormBar(children: PageNode[]): PageNode[] {
	const forms = children.filter((child) => child.type === 'view' && viewTypeOf(child) === 'form');
	if (forms.length !== 1) return children;
	const form = forms[0]!;
	const bar = children.filter((child) => FORM_BAR.has(child.type));
	if (bar.length === 0) return children;
	const moved = { ...form, children: [...bar, ...(form.children ?? [])] };
	return children
		.filter((child) => !FORM_BAR.has(child.type))
		.map((child) => (child === form ? moved : child));
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

	root.children = hoistFormBar(root.children ?? []);

	const page: PageDef = {
		...root,
		type: 'page',
		title: typeof root.title === 'string' ? root.title : 'Untitled',
		route: typeof root.route === 'string' ? root.route : undefined,
		children: root.children ?? []
	};

	return { page, warnings };
}

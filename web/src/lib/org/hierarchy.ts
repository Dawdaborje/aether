import type { OrganizationProfile } from './api';

export interface OrgNode {
	org: OrganizationProfile;
	depth: number;
	children: OrgNode[];
}

/** The organizations below `db` (children, their children, ...) as a tree. */
export function descendantsOf(all: OrganizationProfile[], db: string): OrgNode[] {
	const byParent = new Map<string, OrganizationProfile[]>();
	for (const org of all) {
		if (!org.parent_db_name) continue;
		const list = byParent.get(org.parent_db_name) ?? [];
		list.push(org);
		byParent.set(org.parent_db_name, list);
	}
	const seen = new Set<string>([db]);
	const build = (parent: string, depth: number): OrgNode[] =>
		(byParent.get(parent) ?? [])
			.filter((org) => !seen.has(org.db_name) && seen.add(org.db_name))
			.sort((a, b) => a.name.localeCompare(b.name))
			.map((org) => ({ org, depth, children: build(org.db_name, depth + 1) }));
	return build(db, 1);
}

/** The parents above `db`, nearest first. */
export function ancestorsOf(all: OrganizationProfile[], db: string): OrganizationProfile[] {
	const byDb = new Map(all.map((org) => [org.db_name, org]));
	const chain: OrganizationProfile[] = [];
	const seen = new Set<string>([db]);
	let parent = byDb.get(db)?.parent_db_name;
	while (parent && !seen.has(parent)) {
		seen.add(parent);
		const org = byDb.get(parent);
		if (!org) break;
		chain.push(org);
		parent = org.parent_db_name;
	}
	return chain;
}

/** The tree flattened in display order. */
export function flatten(nodes: OrgNode[]): OrgNode[] {
	return nodes.flatMap((node) => [node, ...flatten(node.children)]);
}

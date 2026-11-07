import type { NavItem } from '$lib/theme/types';
import type { Crumb } from '../crumbs';
import { currentLink } from './nav/navModel';

/** The items from the top of `items` down to `target`, or `[]` when it is not there. */
export function menuPath(items: NavItem[], target: NavItem | null): NavItem[] {
	if (!target) return [];
	for (const item of items) {
		if (item === target) return [item];
		const below = menuPath(item.children ?? [], target);
		if (below.length) return [item, ...below];
	}
	return [];
}

export interface TrailInput {
	/** The organization, when the block shows the scope. */
	org?: string | null;
	/** The current app and its home page. */
	app?: Crumb | null;
	/** The app's page menus, for pages that publish no trail of their own. */
	menus: NavItem[];
	pathname: string;
	/** What the page published (`pageTrail`), or `null`. */
	page: Crumb[] | null;
}

/**
 * The shell's trail: the organization and app (with `scope`), then the page's own crumbs.
 * A page that publishes only its title gets the menu groups above it; a page that publishes
 * nothing gets its place in the app menu. The last crumb is the current page and has no link.
 */
export function shellTrail(input: TrailInput): Crumb[] {
	const path = menuPath(input.menus, currentLink(input.menus, input.pathname)).map((item) => ({
		label: item.label,
		href: item.href
	}));
	let rest: Crumb[];
	if (input.page && input.page.length > 1) rest = input.page;
	else if (input.page?.length === 1) {
		const own = input.page[0]!;
		const above = path.at(-1)?.label === own.label ? path.slice(0, -1) : path;
		rest = [...above, own];
	} else rest = path;

	const app = input.app ?? null;
	// The app's own home page is the app crumb itself.
	rest = rest.filter(
		(crumb, i) =>
			!(app && i === 0 && (crumb.label === app.label || (crumb.href && crumb.href === app.href)))
	);

	const trail: Crumb[] = [
		...(input.org ? [{ label: input.org }] : []),
		...(app ? [app] : []),
		...rest
	];
	const last = trail.at(-1);
	if (last) trail[trail.length - 1] = { label: last.label };
	return trail;
}

import { goto } from '$app/navigation';
import type { NavItem } from '$lib/theme/types';
import { navHref } from '../../nav';

/** Follow a nav item chosen from a menu: app paths in the app, URLs in a new tab. */
export function openItem(item: NavItem) {
	// `navHref` applies the base path to app paths; URLs leave the app.
	// eslint-disable-next-line svelte/no-navigation-without-resolve
	if (item.href?.startsWith('/')) void goto(navHref(item.href));
	else if (item.href) window.open(item.href, '_blank', 'noopener,noreferrer');
}

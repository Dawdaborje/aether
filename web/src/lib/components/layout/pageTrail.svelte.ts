import type { Crumb } from './crumbs';

/**
 * The current page's breadcrumb trail, shared between the page (which knows it) and the
 * shell (whose `breadcrumbs` block may draw it). The page's control panel publishes its
 * crumbs here; while a `breadcrumbs` block is on screen the control panel shows only the
 * page's own title, so the trail is not drawn twice.
 */
class PageTrail {
	/** The crumbs the page published, or `null` when it published none. Raw, so a page can tell its own array apart. */
	crumbs = $state.raw<Crumb[] | null>(null);
	/** How many `breadcrumbs` blocks are mounted. */
	shellBlocks = $state(0);

	get shellDrawsTrail(): boolean {
		return this.shellBlocks > 0;
	}
}

export const pageTrail = new PageTrail();

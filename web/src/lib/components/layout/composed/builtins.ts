import type { LayoutDef } from '$lib/theme/types';

/*
 * The built-in layouts, written in the same JSON a theme plugin ships under
 * `layouts/<name>.json`. A theme that wants "the default, but with a help link" copies
 * this into its own layout and edits it.
 */

const topbarBlocks: NonNullable<LayoutDef['regions']>['topbar'] = {
	sticky: true,
	blocks: [
		{ type: 'home-menu' },
		{ type: 'app-title' },
		{ type: 'spacer' },
		{ type: 'developer-badge', when: 'developer', hide_below: 'sm' },
		{ type: 'notifications', when: 'user' },
		{ type: 'company-switcher', hide_below: 'md' },
		{ type: 'org-switcher', hide_below: 'md' },
		{ type: 'user-menu' }
	]
};

/** Top bar with the app launcher and systray; the current app's pages in a left sidebar. */
export const defaultLayout: LayoutDef = {
	name: 'default',
	label: 'Default',
	chrome: 'composed',
	regions: {
		topbar: topbarBlocks,
		sidebar: { width: '14rem', blocks: [{ type: 'app-menu' }] }
	}
};

/** The developer desk: the default layout reading the desk's navigation. */
export const deskLayout: LayoutDef = {
	...defaultLayout,
	name: 'desk',
	label: 'Developer desk',
	regions: {
		topbar: topbarBlocks,
		sidebar: { width: '14rem', blocks: [{ type: 'app-menu', props: { source: 'desk' } }] }
	}
};

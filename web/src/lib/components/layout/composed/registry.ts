import type { Component } from 'svelte';
import { page } from '$app/state';
import { homePath } from '$lib/auth/landing';
import { pageState } from './pageState.svelte';
import { themeStore } from '$lib/theme';
import NotificationBell from '$lib/notifications/NotificationBell.svelte';
import type { LayoutBlock } from '$lib/theme/types';
import ActionBarBlock from './blocks/ActionBarBlock.svelte';
import CreateBlock from './blocks/CreateBlock.svelte';
import PinnedBlock from './blocks/PinnedBlock.svelte';
import SaveBarBlock from './blocks/SaveBarBlock.svelte';
import SidebarToggleBlock from './blocks/SidebarToggleBlock.svelte';
import SkipLinkBlock from './blocks/SkipLinkBlock.svelte';
import WorkspaceTabsBlock from './blocks/WorkspaceTabsBlock.svelte';
import HomeMenu from '../shell/HomeMenu.svelte';
import UserMenu from '../shell/UserMenu.svelte';
import AppMenuBlock from './blocks/AppMenuBlock.svelte';
import AppTitleBlock from './blocks/AppTitleBlock.svelte';
import BackBlock from './blocks/BackBlock.svelte';
import BrandBlock from './blocks/BrandBlock.svelte';
import BreadcrumbsBlock from './blocks/BreadcrumbsBlock.svelte';
import ColorModeBlock from './blocks/ColorModeBlock.svelte';
import CompanySwitcherBlock from './blocks/CompanySwitcherBlock.svelte';
import DeveloperBadgeBlock from './blocks/DeveloperBadgeBlock.svelte';
import DividerBlock from './blocks/DividerBlock.svelte';
import LinkBlock from './blocks/LinkBlock.svelte';
import NavBlock from './blocks/NavBlock.svelte';
import OrgSwitcherBlock from './blocks/OrgSwitcherBlock.svelte';
import ScopeSwitcherBlock from './blocks/ScopeSwitcherBlock.svelte';
import SearchBlock from './blocks/SearchBlock.svelte';
import SpacerBlock from './blocks/SpacerBlock.svelte';
import TextBlock from './blocks/TextBlock.svelte';
import { navHref } from '../nav';
import type { ShellState } from './shellContext.svelte';

/** Every block gets its definition and whether its region is a column. */
export type BlockProps = { block: LayoutBlock; vertical?: boolean };

export interface BlockSpec {
	component: Component<BlockProps>;
	/** True when the block would draw nothing right now (a region of only empty blocks hides). */
	isEmpty?: (shell: ShellState, block: LayoutBlock) => boolean;
	/** Only takes up room, so it does not keep a region on screen. */
	filler?: boolean;
}

/**
 * The block types a layout may use. Must stay in step with `BLOCK_TYPES` in the
 * server's `plugin_manager/layout_blocks.rs`, which rejects anything else.
 */
export const blockRegistry: Record<string, BlockSpec> = {
	'action-bar': {
		component: ActionBarBlock as unknown as Component<BlockProps>,
		isEmpty: () => !pageState.selection || pageState.selection.count === 0
	},
	'app-menu': {
		component: AppMenuBlock as unknown as Component<BlockProps>,
		isEmpty: (shell, block) => (block.items ?? shell.appMenus).length === 0
	},
	'app-title': { component: AppTitleBlock as unknown as Component<BlockProps> },
	back: {
		component: BackBlock as unknown as Component<BlockProps>,
		// Nothing to go back to on the home page.
		isEmpty: () => page.url.pathname === navHref(homePath())
	},
	brand: { component: BrandBlock as unknown as Component<BlockProps> },
	breadcrumbs: { component: BreadcrumbsBlock as unknown as Component<BlockProps> },
	'color-mode': { component: ColorModeBlock as unknown as Component<BlockProps> },
	'company-switcher': { component: CompanySwitcherBlock as unknown as Component<BlockProps> },
	create: { component: CreateBlock as unknown as Component<BlockProps> },
	'developer-badge': { component: DeveloperBadgeBlock as unknown as Component<BlockProps> },
	divider: { component: DividerBlock as unknown as Component<BlockProps>, filler: true },
	'home-menu': { component: HomeMenu as unknown as Component<BlockProps> },
	link: { component: LinkBlock as unknown as Component<BlockProps> },
	nav: {
		component: NavBlock as unknown as Component<BlockProps>,
		isEmpty: (shell, block) =>
			(
				block.items ??
				(block.props?.source === 'footer' ? (themeStore.theme.nav?.footer ?? []) : shell.nav.items)
			).length === 0
	},
	notifications: { component: NotificationBell as unknown as Component<BlockProps> },
	pinned: { component: PinnedBlock as unknown as Component<BlockProps> },
	'save-bar': {
		component: SaveBarBlock as unknown as Component<BlockProps>,
		isEmpty: () => !pageState.form
	},
	'sidebar-toggle': { component: SidebarToggleBlock as unknown as Component<BlockProps> },
	'skip-link': { component: SkipLinkBlock as unknown as Component<BlockProps>, filler: true },
	'org-switcher': { component: OrgSwitcherBlock as unknown as Component<BlockProps> },
	'scope-switcher': { component: ScopeSwitcherBlock as unknown as Component<BlockProps> },
	search: { component: SearchBlock as unknown as Component<BlockProps> },
	spacer: { component: SpacerBlock as unknown as Component<BlockProps>, filler: true },
	text: { component: TextBlock as unknown as Component<BlockProps> },
	'user-menu': { component: UserMenu as unknown as Component<BlockProps> },
	'workspace-tabs': { component: WorkspaceTabsBlock as unknown as Component<BlockProps> }
};

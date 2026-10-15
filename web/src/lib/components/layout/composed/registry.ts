import type { Component } from 'svelte';
import NotificationBell from '$lib/notifications/NotificationBell.svelte';
import type { LayoutBlock } from '$lib/theme/types';
import HomeMenu from '../shell/HomeMenu.svelte';
import UserMenu from '../shell/UserMenu.svelte';
import AppMenuBlock from './blocks/AppMenuBlock.svelte';
import AppTitleBlock from './blocks/AppTitleBlock.svelte';
import BrandBlock from './blocks/BrandBlock.svelte';
import ColorModeBlock from './blocks/ColorModeBlock.svelte';
import CompanySwitcherBlock from './blocks/CompanySwitcherBlock.svelte';
import DeveloperBadgeBlock from './blocks/DeveloperBadgeBlock.svelte';
import DividerBlock from './blocks/DividerBlock.svelte';
import LinkBlock from './blocks/LinkBlock.svelte';
import NavBlock from './blocks/NavBlock.svelte';
import OrgSwitcherBlock from './blocks/OrgSwitcherBlock.svelte';
import SpacerBlock from './blocks/SpacerBlock.svelte';
import TextBlock from './blocks/TextBlock.svelte';
import type { ShellState } from './shellContext.svelte';

/** Every block gets its definition and whether its region is a column. */
export type BlockProps = { block: LayoutBlock; vertical?: boolean };

export interface BlockSpec {
	component: Component<BlockProps>;
	/** True when the block would draw nothing right now (a region of only empty blocks hides). */
	isEmpty?: (shell: ShellState) => boolean;
	/** Only takes up room, so it does not keep a region on screen. */
	filler?: boolean;
}

/**
 * The block types a layout may use. Must stay in step with `BLOCK_TYPES` in the
 * server's `plugin_manager/layout_blocks.rs`, which rejects anything else.
 */
export const blockRegistry: Record<string, BlockSpec> = {
	'app-menu': {
		component: AppMenuBlock as unknown as Component<BlockProps>,
		isEmpty: (shell) => shell.appMenus.length === 0
	},
	'app-title': { component: AppTitleBlock as unknown as Component<BlockProps> },
	brand: { component: BrandBlock as unknown as Component<BlockProps> },
	'color-mode': { component: ColorModeBlock as unknown as Component<BlockProps> },
	'company-switcher': { component: CompanySwitcherBlock as unknown as Component<BlockProps> },
	'developer-badge': { component: DeveloperBadgeBlock as unknown as Component<BlockProps> },
	divider: { component: DividerBlock as unknown as Component<BlockProps>, filler: true },
	'home-menu': { component: HomeMenu as unknown as Component<BlockProps> },
	link: { component: LinkBlock as unknown as Component<BlockProps> },
	nav: {
		component: NavBlock as unknown as Component<BlockProps>,
		isEmpty: (shell) => shell.nav.items.length === 0
	},
	notifications: { component: NotificationBell as unknown as Component<BlockProps> },
	'org-switcher': { component: OrgSwitcherBlock as unknown as Component<BlockProps> },
	spacer: { component: SpacerBlock as unknown as Component<BlockProps>, filler: true },
	text: { component: TextBlock as unknown as Component<BlockProps> },
	'user-menu': { component: UserMenu as unknown as Component<BlockProps> }
};

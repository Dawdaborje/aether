import type { LayoutBlock } from '$lib/theme/types';

/** What decides whether a block shows for the signed-in person. */
export interface Viewer {
	isUser: boolean;
	isDeveloper: boolean;
	canManageUsers: boolean;
	canManageOrg: boolean;
	roles: string[];
}

/** Whether `block` shows for `viewer`: its `when` and, if given, one of its `roles`. */
export function blockVisible(block: LayoutBlock, viewer: Viewer): boolean {
	switch (block.when) {
		case 'user':
			if (!viewer.isUser) return false;
			break;
		case 'developer':
			if (!viewer.isDeveloper) return false;
			break;
		case 'manager':
			if (!viewer.canManageUsers && !viewer.isDeveloper) return false;
			break;
		case 'admin':
			if (!viewer.canManageOrg && !viewer.isDeveloper) return false;
			break;
	}
	if (block.roles?.length && !viewer.isDeveloper) {
		return block.roles.some((role) => viewer.roles.includes(role));
	}
	return true;
}

/**
 * Tailwind classes hiding a block below a breakpoint. Written out in full so Tailwind's
 * scanner sees them.
 */
const HIDE_BELOW: Record<NonNullable<LayoutBlock['hide_below']>, string> = {
	sm: 'max-sm:hidden',
	md: 'max-md:hidden',
	lg: 'max-lg:hidden',
	xl: 'max-xl:hidden'
};

export function hideBelowClass(block: LayoutBlock): string {
	return block.hide_below ? HIDE_BELOW[block.hide_below] : '';
}

/** A string prop, or `fallback`. */
export function textProp(block: LayoutBlock, key: string, fallback = ''): string {
	const value = block.props?.[key];
	return typeof value === 'string' ? value : fallback;
}

/** A boolean prop, or `fallback`. */
export function flagProp(block: LayoutBlock, key: string, fallback = false): boolean {
	const value = block.props?.[key];
	return typeof value === 'boolean' ? value : fallback;
}

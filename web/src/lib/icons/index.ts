import type { Component } from 'svelte';
import { themeStore } from '$lib/theme/themeStore.svelte';
import type { ThemeIcon } from '$lib/theme/types';
import { defaultIcons } from './registry';

export { defaultIcons, type IconName } from './registry';

/** The default set's icon for `name`, or undefined when it has none. */
export function defaultIcon(
	name: string | null | undefined
): Component<{ class?: string }> | undefined {
	return name && Object.hasOwn(defaultIcons, name)
		? defaultIcons[name as keyof typeof defaultIcons]
		: undefined;
}

/** The active theme's icon for `name`, from its sprite, if it has one. */
export function themeIcon(name: string | null | undefined): ThemeIcon | undefined {
	const icons = themeStore.theme.shell?.icons;
	return name && icons && Object.hasOwn(icons, name) ? icons[name] : undefined;
}

/**
 * `name` when the theme or the default set has an icon for it, otherwise undefined. Draw it
 * with `<AppIcon name={…} />`. Reactive to the active theme.
 */
export function iconRef(name: string | null | undefined): string | undefined {
	return name && (themeIcon(name) || defaultIcon(name)) ? name : undefined;
}

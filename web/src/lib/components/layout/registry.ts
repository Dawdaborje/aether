import type { Component, Snippet } from 'svelte';
import BareLayout from './bare/bareLayout.svelte';
import CustomNavbarLayout from './custom/customNavbarLayout.svelte';
import DefaultLayout from './default/defaultLayout.svelte';
import NavbarLayout from './navbar/navbarLayout.svelte';
import DeskLayout from './desk/deskLayout.svelte';

type LayoutComponent = Component<{ children: Snippet }>;

/**
 * Built-in layouts a theme (`"layout": "…"`) or page (`<page layout="…">`) can
 * choose by name. Plugin-shipped layouts use `layouts/<name>.json` +
 * `CustomLayout` instead (see `theme.layoutDef`).
 */
export const layoutRegistry: Record<string, LayoutComponent> = {
	default: DefaultLayout as LayoutComponent,
	desk: DeskLayout as LayoutComponent,
	navbar: NavbarLayout as LayoutComponent,
	custom: CustomNavbarLayout as LayoutComponent,
	bare: BareLayout as LayoutComponent
};

/** The component for `name`; an unknown name falls back to the default layout. */
export function layoutFor(name: string): LayoutComponent {
	const layout = layoutRegistry[name];
	if (!layout) {
		console.warn(
			`Unknown layout "${name}"; using "default". Known: ${Object.keys(layoutRegistry).join(', ')}`
		);
		return layoutRegistry.default;
	}
	return layout;
}

import type { Component, Snippet } from 'svelte';
import BareLayout from './bare/bareLayout.svelte';
import CustomLayout from './custom/customLayout.svelte';
import DefaultLayout from './default/defaultLayout.svelte';
import DeskLayout from './desk/deskLayout.svelte';

type LayoutComponent = Component<{ children: Snippet }>;

/**
 * Layouts a theme (`"layout": "…"` in its theme.json) or a page
 * (`<page layout="…">`) can choose by name. To add one, create the component
 * and list it here.
 */
export const layoutRegistry: Record<string, LayoutComponent> = {
	default: DefaultLayout as LayoutComponent,
	desk: DeskLayout as LayoutComponent,
	custom: CustomLayout as LayoutComponent,
	bare: BareLayout as LayoutComponent
};

/** The component for `name`; an unknown name falls back to the default layout. */
export function layoutFor(name: string): LayoutComponent {
	const layout = layoutRegistry[name];
	if (!layout) {
		console.warn(`Unknown layout "${name}"; using "default". Known: ${Object.keys(layoutRegistry).join(', ')}`);
		return layoutRegistry.default;
	}
	return layout;
}

<script lang="ts">
	import { defaultIcon, themeIcon } from './index';

	/**
	 * An icon by name: the active theme's symbol when its sprite has one, otherwise the
	 * default (Carbon) icon of that name. Draws nothing for an unknown name. Every icon in
	 * the app is drawn through this component, so a theme can replace any of them (see
	 * "Icons" in docs/architecture/themes.md).
	 */
	let {
		name,
		class: className = '',
		...rest
	}: {
		name: string | null | undefined;
		class?: string;
		/** Other attributes (`aria-*`, `data-*`, `style`) go to the drawn `<svg>`. */
		[attribute: string]: unknown;
	} = $props();

	const sprite = $derived(themeIcon(name));
	const Default = $derived(defaultIcon(name));
</script>

{#if sprite}
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="16"
		height="16"
		fill="currentColor"
		{...sprite.attrs}
		viewBox={sprite.view_box}
		class={className}
		aria-hidden="true"
		focusable="false"
		data-icon={name}
		{...rest}
	>
		<!-- The server rebuilt this markup from plain shapes and checked values (theme_icons.rs). -->
		<!-- eslint-disable-next-line svelte/no-at-html-tags -->
		{@html sprite.body}
	</svg>
{:else if Default}
	<Default class={className} {...rest} />
{/if}

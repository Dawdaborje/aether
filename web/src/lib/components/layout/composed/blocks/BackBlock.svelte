<script lang="ts">
	import { goto } from '$app/navigation';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { homePath } from '$lib/auth/landing';
	import type { LayoutBlock } from '$lib/theme/types';
	import { navHref } from '../../nav';
	import { textProp } from '../visibility';

	let { block }: { block: LayoutBlock; vertical?: boolean } = $props();
	const label = $derived(textProp(block, 'label', 'Back'));

	function back() {
		// The browser's own history when there is some; the home page when the page was opened directly.
		if (window.history.length > 1) window.history.back();
		// eslint-disable-next-line svelte/no-navigation-without-resolve -- navHref() applies the base path
		else void goto(navHref(homePath()));
	}
</script>

<!-- A back arrow for full-screen apps. It only shows away from the home page (see the registry). -->
<button
	type="button"
	data-part="shell.back"
	aria-label={label}
	title={label}
	onclick={back}
	class="flex size-8 shrink-0 items-center justify-center rounded-button text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
>
	<AppIcon name="arrow-left" class="size-4" />
</button>

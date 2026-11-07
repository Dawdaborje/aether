<script lang="ts">
	import type { LayoutBlock } from '$lib/theme/types';
	import { textProp } from '../visibility';

	let { block }: { block: LayoutBlock } = $props();
	const label = $derived(textProp(block, 'label', 'Skip to content'));
	const target = $derived(textProp(block, 'target', 'content'));

	function skip(event: MouseEvent) {
		// A hash link would change the address (and the base path); move focus instead.
		event.preventDefault();
		const element = document.getElementById(target);
		element?.focus();
		element?.scrollIntoView();
	}
</script>

<!-- Hidden until the first Tab, then the first thing a keyboard user can reach. -->
<a
	href="#{target}"
	data-part="shell.skip-link"
	onclick={skip}
	class="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-[100] focus:rounded-md focus:bg-primary focus:px-3 focus:py-2 focus:text-sm focus:font-medium focus:text-primary-foreground"
	>{label}</a
>

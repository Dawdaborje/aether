<script lang="ts">
	import type { LayoutBlock } from '$lib/theme/types';
	import { flagProp, textProp } from '../visibility';

	let { block }: { block: LayoutBlock } = $props();
</script>

<span
	class="truncate px-1.5 text-sm {flagProp(block, 'muted', true)
		? 'text-region-muted-foreground'
		: ''}">{textProp(block, 'text')}</span
>

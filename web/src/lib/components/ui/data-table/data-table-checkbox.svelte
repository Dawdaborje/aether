<script lang="ts">
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';

	let {
		checked = false,
		indeterminate = false,
		onCheckedChange,
		'aria-label': ariaLabel
	}: {
		checked?: boolean;
		indeterminate?: boolean;
		onCheckedChange?: (value: boolean) => void;
		'aria-label'?: string;
	} = $props();
</script>

<!-- stopPropagation: row click (open) must not fire when toggling selection -->
<div
	onclick={(event) => event.stopPropagation()}
	onkeydown={(event) => event.stopPropagation()}
	role="presentation"
>
	<Checkbox
		checked={checked === true}
		{indeterminate}
		onCheckedChange={(value) => onCheckedChange?.(!!value)}
		aria-label={ariaLabel}
	/>
</div>

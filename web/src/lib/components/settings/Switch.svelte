<script lang="ts">
	import { Switch as UiSwitch } from '$lib/components/ui/switch';

	let {
		checked,
		disabled = false,
		label,
		onchange
	}: {
		checked: boolean;
		disabled?: boolean;
		/** Accessible name. */
		label: string;
		onchange: (checked: boolean) => void;
	} = $props();
</script>

<UiSwitch
	{checked}
	{disabled}
	aria-label={label}
	onCheckedChange={(next) => {
		if (next !== undefined) onchange(next);
	}}
/>

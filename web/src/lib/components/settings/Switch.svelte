<script lang="ts">
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

<button
	type="button"
	role="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	onclick={() => onchange(!checked)}
	class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border border-transparent transition-colors focus-visible:outline-2 disabled:cursor-not-allowed disabled:opacity-50 {checked
		? 'bg-primary'
		: 'bg-input'}"
>
	<span
		class="pointer-events-none block size-5 rounded-full bg-white shadow-sm ring-0 transition-transform {checked
			? 'translate-x-5'
			: 'translate-x-0.5'}"
	></span>
</button>

<script lang="ts">
	let {
		label,
		field,
		sortable,
		mark,
		onToggle
	}: {
		label: string;
		field: string;
		sortable: boolean;
		mark: string;
		onToggle: (field: string) => void;
	} = $props();
</script>

{#if sortable}
	<button
		type="button"
		class="inline-flex items-center gap-0.5 font-medium hover:text-foreground"
		onclick={() => onToggle(field)}
	>
		{label}{mark}
	</button>
{:else}
	{label}
{/if}

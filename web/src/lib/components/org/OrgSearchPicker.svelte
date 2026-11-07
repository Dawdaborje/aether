<script lang="ts">
	import { SearchPicker } from '$lib/components/ui/search-picker';
	import type { OrgChoice } from '$lib/org/orgStore.svelte';

	let {
		organizations,
		value = $bindable(null),
		currentDb = null,
		placeholder = 'Select organization'
	}: {
		organizations: OrgChoice[];
		value?: string | null;
		currentDb?: string | null;
		placeholder?: string;
	} = $props();

	const items = $derived(
		organizations.map((org) => ({
			id: org.db_name,
			label: org.name,
			sublabel: org.db_name
		}))
	);
</script>

<SearchPicker
	{items}
	bind:value
	preferId={currentDb}
	label="Organization"
	{placeholder}
	searchPlaceholder="Search organizations…"
	entityLabel="organizations"
	icon="building-2"
	class="max-w-xs"
/>

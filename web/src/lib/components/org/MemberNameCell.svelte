<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';

	let {
		name,
		isOwner = false
	}: {
		name: string;
		isOwner?: boolean;
	} = $props();
</script>

<span class="inline-flex flex-wrap items-center gap-1.5">
	{name}
	{#if isOwner}
		<Badge>Owner</Badge>
	{/if}
</span>

<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';

	let {
		label,
		variant = 'default'
	}: {
		label: string;
		variant?: 'default' | 'secondary' | 'destructive' | 'outline';
	} = $props();
</script>

{#if label}
	<Badge {variant}>{label}</Badge>
{/if}

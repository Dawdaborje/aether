<script lang="ts">
	import { defaultLayout } from '../composed/builtins';
	import ComposedLayout from '../composed/ComposedLayout.svelte';

	let { children } = $props();
</script>

<ComposedLayout def={defaultLayout}>
	{@render children()}
</ComposedLayout>

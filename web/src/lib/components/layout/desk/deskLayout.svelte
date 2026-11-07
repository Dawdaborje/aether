<script lang="ts">
	import { deskLayout } from '../composed/builtins';
	import ComposedLayout from '../composed/ComposedLayout.svelte';

	let { children } = $props();
</script>

<!-- The developer desk: the default layout with the desk's navigation. -->
<ComposedLayout def={deskLayout}>
	{@render children()}
</ComposedLayout>

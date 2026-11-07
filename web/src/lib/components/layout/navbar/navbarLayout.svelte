<script lang="ts">
	import { navbarLayout } from '../composed/builtins';
	import ComposedLayout from '../composed/ComposedLayout.svelte';

	let { children } = $props();
</script>

<!-- The app-switcher layout: launcher, app name and the app's menus in the top bar. -->
<ComposedLayout def={navbarLayout}>
	{@render children()}
</ComposedLayout>

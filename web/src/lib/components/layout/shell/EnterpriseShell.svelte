<script lang="ts">
	import type { ThemeNav } from '$lib/theme/types';
	import Navbar from './Navbar.svelte';

	let { nav, children }: { nav: ThemeNav; children: import('svelte').Snippet } = $props();
</script>

<!--
	The built-in shell, in the manner of Odoo: a slim top bar (home menu, app name, menus,
	systray) over a quiet canvas that pages fill with their control panel and sheets.
	The default layout and the developer desk both render it, with different menus.
-->
<div class="flex min-h-screen flex-col bg-canvas">
	<Navbar {nav} />
	<main class="flex min-h-0 flex-1 flex-col">
		{@render children()}
	</main>
</div>

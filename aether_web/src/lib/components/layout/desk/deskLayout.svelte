<script lang="ts">
	import DeskSideBar from './deskSideBar.svelte';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';

	let { children } = $props();
</script>

<Sidebar.Provider class="min-h-screen">
	<DeskSideBar />
	<Sidebar.Inset class="min-h-screen bg-background">
		<header
			class="flex h-12 items-center gap-2 border-b border-border bg-card px-4"
		>
			<Sidebar.Trigger />
			<div class="text-sm text-muted-foreground">Enterprise workspace</div>
		</header>
		<main class="min-h-[calc(100vh-3rem)] flex-1 overflow-auto">
			{@render children()}
		</main>
	</Sidebar.Inset>
</Sidebar.Provider>

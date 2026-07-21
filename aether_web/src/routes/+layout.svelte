<script lang="ts">
	import { onMount } from 'svelte';
	import { ModeWatcher } from 'mode-watcher';
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { themeStore } from '$lib/theme';

	let { children } = $props();

	onMount(() => {
		void themeStore.loadFromApi('');
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Aether</title>
</svelte:head>

<ModeWatcher />

<div class="h-screen w-full bg-background text-foreground">
	{@render children()}
</div>

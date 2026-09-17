<script>
	import DefaultNavbar from './defaultNavbar.svelte';
	import DefaultSideBar from './defaultSideBar.svelte';

	let { children } = $props();
</script>

<div class="flex w-full flex-col">
	<DefaultNavbar />
	<div class="flex">
		<DefaultSideBar />
		<main class="p-4">
			{@render children()}
		</main>
	</div>
</div>

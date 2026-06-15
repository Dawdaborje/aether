<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import DefaultLayout from '$lib/components/layout/default/defaultLayout.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();

	let hasHomePage = $state(false);

	onMount(() => {
		if (!hasHomePage) {
			goto(resolve('/apps'));
		}
	});
</script>

<DefaultLayout>
	{@render children()}
</DefaultLayout>

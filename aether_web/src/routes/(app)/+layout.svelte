<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import DefaultLayout from '$lib/components/layout/default/defaultLayout.svelte';
	import { fetchMe } from '$lib/auth/api';
	import { onMount } from 'svelte';

	let { children } = $props();

	let ready = $state(false);
	let hasHomePage = $state(false);

	onMount(async () => {
		const me = await fetchMe();
		if (!me) {
			goto(resolve('/login'));
			return;
		}
		ready = true;
		if (!hasHomePage) {
			goto(resolve('/apps'));
		}
	});
</script>

{#if ready}
	<DefaultLayout>
		{@render children()}
	</DefaultLayout>
{/if}

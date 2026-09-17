<script>
	import { currentSystemNavParent } from '$lib/systemStore';
	import { onMount } from 'svelte';

	onMount(() => {
		currentSystemNavParent.subscribe((value) => {
			if (value != 'apps') {
				currentSystemNavParent.set('apps');
			}
		});
	});
</script>

<h2>Welcome to the apps page!</h2>

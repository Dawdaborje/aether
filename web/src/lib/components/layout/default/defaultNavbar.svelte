<script lang="ts">
	import { appsNavItems, currentSystemNavParent } from '$lib/systemStore';
	import type { SystemNav } from '$lib/types/sysemNav';
	import { onMount } from 'svelte';

	let navSystem: SystemNav | null | undefined = $state();

	onMount(() => {
		currentSystemNavParent.subscribe((value) => {
			if (value == 'apps') {
				navSystem = appsNavItems;
			}
		});
	});
</script>

<nav class="flex justify-between bg-amber-500 p-5 text-white">
	{#if navSystem}
		<h1 class="font-bold">{navSystem.header}</h1>

		<ul>
			{#if navSystem.items.length > 0}
				{#each navSystem.items as item (item.href)}
					<li>
						<a href={item.href}>
							{item.label}
						</a>
					</li>
				{/each}
			{/if}
		</ul>
	{/if}
</nav>

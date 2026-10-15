<script lang="ts">
	import MoonIcon from '@lucide/svelte/icons/moon';
	import SunIcon from '@lucide/svelte/icons/sun';
	import { themeStore } from '$lib/theme';
</script>

<!-- One-click light / dark switch (the user menu also offers "system"). -->
<button
	type="button"
	onclick={() => themeStore.setUserMode(themeStore.isDark ? 'light' : 'dark')}
	aria-label={themeStore.isDark ? 'Use light mode' : 'Use dark mode'}
	title={themeStore.isDark ? 'Light mode' : 'Dark mode'}
	class="flex size-8 items-center justify-center rounded-md text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring"
>
	{#if themeStore.isDark}<SunIcon class="size-4" />{:else}<MoonIcon class="size-4" />{/if}
</button>

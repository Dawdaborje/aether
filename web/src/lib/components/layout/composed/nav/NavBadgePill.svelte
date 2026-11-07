<script lang="ts">
	import type { NavBadge } from '$lib/theme/types';
	import { badgeFunction, navBadges } from './navBadges.svelte';

	let { badge, class: className = '' }: { badge: NavBadge | undefined; class?: string } = $props();

	const fn = $derived(badgeFunction(badge));
	const text = $derived(
		typeof badge === 'string' ? badge : fn ? (navBadges.values[fn] ?? null) : null
	);

	$effect(() => {
		if (fn) return navBadges.use(fn);
	});
</script>

<!-- A count or a short label beside a nav entry. Nothing when the count is zero. -->
{#if text}
	<span
		data-part="nav.badge"
		class="ml-auto inline-flex h-5 min-w-5 shrink-0 items-center justify-center rounded-chip bg-region-active px-1.5 text-[11px] leading-none font-medium text-region-foreground tabular-nums {className}"
		>{text}</span
	>
{/if}

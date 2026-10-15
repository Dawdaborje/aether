<script lang="ts">
	import { bridgeLogoUrl } from '$lib/bridges/logos';

	let {
		featureKey,
		label,
		size = 'md'
	}: {
		featureKey: string;
		label: string;
		size?: 'md' | 'lg';
	} = $props();

	let failed = $state(false);
	const box = $derived(size === 'lg' ? 'size-14' : 'size-11');
	const img = $derived(size === 'lg' ? 'size-14' : 'size-11');
	const text = $derived(size === 'lg' ? 'text-xl' : 'text-sm');

	$effect(() => {
		void featureKey;
		failed = false;
	});
</script>

<span
	class="flex {box} shrink-0 items-center justify-center overflow-hidden rounded-xl bg-muted text-muted-foreground ring-1 ring-border"
	aria-hidden="true"
>
	{#if !failed}
		<img
			src={bridgeLogoUrl(featureKey)}
			alt=""
			class="{img} object-cover"
			onerror={() => (failed = true)}
		/>
	{:else}
		<span class="{text} font-semibold text-primary">{label.charAt(0).toUpperCase()}</span>
	{/if}
</span>

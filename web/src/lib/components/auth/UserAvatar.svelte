<script lang="ts">
	import * as Avatar from '$lib/components/ui/avatar/index.js';

	let {
		name = 'Account',
		avatarUrl = null,
		cacheKey = '',
		class: className = 'size-9',
		fallbackClass = ''
	}: {
		name?: string;
		avatarUrl?: string | null;
		/** Bust the browser cache after an upload. */
		cacheKey?: string | number | null;
		class?: string;
		fallbackClass?: string;
	} = $props();

	const initials = $derived(
		name
			.split(/[\s@._-]+/)
			.filter(Boolean)
			.slice(0, 2)
			.map((part) => part.charAt(0).toUpperCase())
			.join('') || '?'
	);

	const src = $derived.by(() => {
		const url = avatarUrl?.trim();
		if (!url) return '';
		if (cacheKey === undefined || cacheKey === null || cacheKey === '') return url;
		const sep = url.includes('?') ? '&' : '?';
		return `${url}${sep}t=${cacheKey}`;
	});
</script>

<Avatar.Root class={className}>
	{#if src}
		<Avatar.Image src={src} alt={name} class="object-cover" />
	{/if}
	<Avatar.Fallback class={fallbackClass}>{initials}</Avatar.Fallback>
</Avatar.Root>

<script lang="ts">
	import { resolve } from '$app/paths';

	let {
		logoUrl = null,
		name = '',
		cacheKey = '',
		class: className = 'size-11 rounded-lg',
		alt = ''
	}: {
		/** Uploaded logo URL (`/api/ui/organizations/…/logo`), or null for the default mark. */
		logoUrl?: string | null;
		/** Used for the image alt text when `alt` is empty. */
		name?: string;
		/** Bust the browser cache after an upload (e.g. `date_updated`). */
		cacheKey?: string | number | null;
		class?: string;
		alt?: string;
	} = $props();

	const defaultSrc = $derived(resolve('/org-default.svg'));
	const src = $derived.by(() => {
		const url = logoUrl?.trim();
		if (!url) return defaultSrc;
		if (cacheKey === undefined || cacheKey === null || cacheKey === '') return url;
		const sep = url.includes('?') ? '&' : '?';
		return `${url}${sep}t=${cacheKey}`;
	});

	let failed = $state(false);
	const showUploaded = $derived(Boolean(logoUrl?.trim()) && !failed);

	$effect(() => {
		void logoUrl;
		void cacheKey;
		failed = false;
	});
</script>

<span
	class="inline-flex shrink-0 items-center justify-center overflow-hidden bg-muted ring-1 ring-border {className}"
	aria-hidden={alt ? undefined : true}
>
	<img
		src={showUploaded ? src : defaultSrc}
		alt={alt || (name ? `${name} logo` : '')}
		class="size-full {showUploaded ? 'object-cover' : 'object-contain'}"
		onerror={() => {
			if (showUploaded) failed = true;
		}}
	/>
</span>

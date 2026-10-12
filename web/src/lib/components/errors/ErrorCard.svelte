<script lang="ts">
	import { themeStore } from '$lib/theme';
	import { errorCopy } from './copy';
	import { errorSetFor } from './registry';
	import type { ErrorCardProps } from './types';

	let { title, message }: ErrorCardProps = $props();

	const def = $derived(themeStore.theme.errorPagesDef);
	const setName = $derived(def?.variant || themeStore.theme.errorPages);
	const Card = $derived(errorSetFor(setName).card);
	const words = $derived.by(() => {
		const override = def?.card;
		return {
			title: title || override?.title || errorCopy(500, {}).title,
			message: message || override?.message || errorCopy(500, {}).message
		};
	});
</script>

<Card title={words.title} message={words.message} />

<script lang="ts">
	import { themeStore } from '$lib/theme';
	import { errorSetFor } from './registry';
	import type { ErrorCardProps } from './types';

	let { title, message }: ErrorCardProps = $props();

	const Card = $derived(errorSetFor(themeStore.theme.errorPages).card);
</script>

<Card {title} {message} />

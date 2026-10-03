<script lang="ts">
	import { themeStore } from '$lib/theme';
	import { errorSetFor, pageFor } from './registry';
	import type { ErrorPageProps } from './types';

	let { status, title, message, detail, action }: ErrorPageProps = $props();

	// The look comes from the active theme's error-page set.
	const View = $derived(pageFor(errorSetFor(themeStore.theme.errorPages), status));
</script>

<View {status} {title} {message} {detail} {action} />

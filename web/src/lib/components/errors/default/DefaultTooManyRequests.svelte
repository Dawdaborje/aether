<script lang="ts">
	import { errorCopy } from '../copy';
	import type { ErrorPageProps } from '../types';
	import DefaultErrorView from './DefaultErrorView.svelte';

	let { status = 429, title, message, detail, action }: Partial<ErrorPageProps> = $props();
	const words = $derived(errorCopy(status, { title, message }));
</script>

<DefaultErrorView {status} title={words.title} message={words.message} {detail} {action} />

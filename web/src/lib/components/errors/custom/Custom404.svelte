<script lang="ts">
	import { errorCopy } from '../copy';
	import type { ErrorPageProps } from '../types';
	import CustomErrorView from './CustomErrorView.svelte';

	let { status = 404, title, message, detail, action }: Partial<ErrorPageProps> = $props();
	const words = $derived(errorCopy(status, { title, message }));
</script>

<CustomErrorView {status} title={words.title} message={words.message} {detail} {action} />

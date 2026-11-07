<script lang="ts">
	import { themeStore } from '$lib/theme';
	import { errorCopy } from './copy';
	import { errorSetFor, pageFor } from './registry';
	import type { ErrorPageProps } from './types';

	let { status, title, message, detail, action }: ErrorPageProps = $props();

	const def = $derived(themeStore.theme.errorPagesDef);
	const setName = $derived(def?.variant || themeStore.theme.errorPages);
	const View = $derived(pageFor(errorSetFor(setName), status));

	const words = $derived.by(() => {
		const override =
			status === 404
				? def?.not_found
				: status === 401 || status === 403
					? def?.forbidden
					: status === 429
						? def?.too_many_requests
						: def?.server_error;
		const base = errorCopy(status, { title, message });
		return {
			title: title || override?.title || base.title,
			message: message || override?.message || base.message
		};
	});
</script>

<View {status} title={words.title} message={words.message} {detail} {action} />

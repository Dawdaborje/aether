<script lang="ts">
	import { page } from '$app/state';
	import ErrorPage from '$lib/components/errors/ErrorPage.svelte';
</script>

<!-- SvelteKit's error page for anything that fails outside a plugin page: a
route that cannot load, an unexpected exception. The look comes from the
active theme's error-page set. -->
<ErrorPage status={page.status} message={page.error?.message} />

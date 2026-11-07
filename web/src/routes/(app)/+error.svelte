<script lang="ts">
	import { page } from '$app/state';
	import ErrorPage from '$lib/components/errors/ErrorPage.svelte';
</script>

<!-- Errors inside the app are shown within the themed application shell. -->
<ErrorPage status={page.status} message={page.error?.message} />

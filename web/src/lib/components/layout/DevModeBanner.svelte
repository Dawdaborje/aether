<script lang="ts">
	import { brandingStore } from '$lib/branding/brandingStore.svelte';
</script>

<!--
	A strip along the bottom edge, outside the page layout so it never moves or covers
	the app: shown on every screen, the login page included, while the server runs in
	development mode (`is_development_mode = true` in aether.toml).
-->
{#if brandingStore.developmentMode}
	<div
		role="status"
		data-testid="dev-mode-banner"
		title="Session cookies are sent without the Secure flag. Set is_development_mode = false in aether.toml for production."
		class="pointer-events-none fixed inset-x-0 bottom-0 z-[100] flex justify-center"
	>
		<span
			class="rounded-t-md bg-amber-500 px-3 py-0.5 text-[11px] font-semibold tracking-wide text-amber-950 shadow-md"
		>
			Development mode · not for production
		</span>
	</div>
{/if}

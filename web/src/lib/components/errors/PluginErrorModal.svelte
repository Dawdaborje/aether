<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { onMount } from 'svelte';
	import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';
	import { pageError, type ErrorKind } from '$lib/pages/pageContext.svelte';

	const TITLES: Record<ErrorKind, string> = {
		validation: 'Validation error',
		server: 'Server error',
		network: 'Connection error',
		permission: 'Permission error',
		not_found: 'Not found',
		rate_limit: 'Too many requests',
		client: 'Request error',
		plugin_load: 'Plugin failed to load'
	};

	// The server tells the browser when a plugin it is watching did not load.
	onMount(() =>
		notificationsStore.onEvent((event) => {
			if (event.plugin !== 'aether' || event.event !== 'dev.error') return;
			const raw = (event.payload ?? {}) as { plugin?: unknown; message?: unknown };
			pageError.current = {
				kind: 'plugin_load',
				status: 0,
				message: String(raw.message ?? 'The plugin could not be loaded.'),
				plugin: String(raw.plugin ?? ''),
				fn: ''
			};
		})
	);

	const err = $derived(pageError.current);
	let open = $state(false);
	$effect(() => {
		open = err !== null;
	});
	$effect(() => {
		if (!open) pageError.current = null;
	});
</script>

<!-- Above page modals (same default z-50) so a failed picker/load is still visible. -->
<Dialog.Root bind:open>
	<Dialog.Content class="z-[100]" overlayClass="z-[100]">
		<Dialog.Header>
			<Dialog.Title>{err ? TITLES[err.kind] : ''}</Dialog.Title>
			<Dialog.Description>
				{#if err}
					<span class="font-mono text-xs">
						type: {err.kind}{err.status ? ` · HTTP ${err.status}` : ''} · {err.plugin}{err.fn
							? `/${err.fn}`
							: ''}
					</span>
				{/if}
			</Dialog.Description>
		</Dialog.Header>
		<p class="max-h-64 overflow-y-auto py-2 text-sm break-words whitespace-pre-wrap text-foreground" role="alert">{err?.message}</p>
		{#if err?.detail}
			<div class="rounded-md border border-border bg-muted/40 p-3">
				<p class="mb-1 text-xs font-medium text-muted-foreground">
					Developer details: function
					<span class="font-mono">{err.plugin}.{err.fn}</span>
				</p>
				<pre
					class="max-h-48 overflow-auto font-mono text-xs break-words whitespace-pre-wrap">{err.detail}</pre>
			</div>
		{/if}
		<Dialog.Footer>
			<Button onclick={() => (open = false)}>Close</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

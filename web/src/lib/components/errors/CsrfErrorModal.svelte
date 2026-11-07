<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';

	/**
	 * Explains a request the server refused as possibly forged (`403 { error: "csrf" }`),
	 * instead of showing the bare code. `reason` comes from `CsrfError`.
	 */
	let {
		reason = $bindable(null)
	}: {
		reason?: 'origin' | 'token' | null;
	} = $props();

	let open = $state(false);
	$effect(() => {
		open = reason !== null;
	});
	$effect(() => {
		if (!open) reason = null;
	});

	const origin = typeof window === 'undefined' ? '' : window.location.origin;
</script>

<Dialog.Root bind:open>
	<Dialog.Content>
		{#if reason === 'origin'}
			<Dialog.Header>
				<Dialog.Title>This address can't be used to sign in</Dialog.Title>
				<Dialog.Description>
					You opened Aether at <code class="font-mono text-foreground">{origin}</code>, and the
					server only accepts sign-ins from the addresses it trusts. Open Aether at its usual
					address and try again.
				</Dialog.Description>
			</Dialog.Header>
			<p class="text-sm text-muted-foreground">
				If you run this server, add <code class="font-mono text-foreground">"{origin}"</code> to
				<code class="font-mono text-foreground">csrf_trusted_origins</code> in the
				<code class="font-mono text-foreground">[server]</code> section of
				<code class="font-mono text-foreground">aether.toml</code>, then restart it.
			</p>
			<Dialog.Footer>
				<Button onclick={() => (open = false)}>Close</Button>
			</Dialog.Footer>
		{:else}
			<Dialog.Header>
				<Dialog.Title>Your sign-in page has expired</Dialog.Title>
				<Dialog.Description>
					The security check that protects sign-in didn't pass. This usually happens when the page
					has been open for a long time or your cookies were cleared. Reload the page and sign in
					again.
				</Dialog.Description>
			</Dialog.Header>
			<p class="text-sm text-muted-foreground">
				If this keeps happening, check that your browser allows cookies for this site.
			</p>
			<Dialog.Footer>
				<Button variant="secondary" onclick={() => (open = false)}>Close</Button>
				<Button onclick={() => window.location.reload()}>Reload page</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>

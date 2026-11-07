<script lang="ts">
	import { onMount } from 'svelte';
	import { apiFetch } from '$lib/api/client';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import type { PageNode } from '$lib/dsl/types';
	import { text } from '$lib/pages/pageContext.svelte';

	/**
	 * `<login bridge="jmap" title="Your mailbox" help="Your user name and password on the mail server."/>`: where a person
	 * connects their own login for a bridge that works on their behalf (their mailbox). The kernel checks the login with the
	 * service before keeping it, stores the password encrypted, and never shows it again; plugins never see it. Offers
	 * "Disconnect" when a login is kept. After connecting or disconnecting the page moves to `redirect` if there is one.
	 */
	let { node }: { node: PageNode } = $props();

	const bridge = $derived(text(node.bridge) ?? '');
	let connected = $state(false);
	let name = $state('');
	let username = $state('');
	let secret = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);
	let done = $state<string | null>(null);

	async function load() {
		const r = await apiFetch(`/api/ui/bridge-credentials/${encodeURIComponent(bridge)}`);
		if (!r.ok) {
			error =
				((await r.json().catch(() => ({}))) as { error?: string }).error ??
				'That could not be read.';
			return;
		}
		const body = (await r.json()) as { connected: boolean; username: string | null };
		connected = body.connected;
		name = body.username ?? '';
	}

	async function save(event: Event) {
		event.preventDefault();
		busy = true;
		error = null;
		done = null;
		const r = await apiFetch(`/api/ui/bridge-credentials/${encodeURIComponent(bridge)}`, {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ username, secret })
		});
		busy = false;
		secret = '';
		if (!r.ok) {
			error =
				((await r.json().catch(() => ({}))) as { error?: string }).error ?? 'That did not work.';
			return;
		}
		done = 'Connected.';
		await load();
	}

	async function disconnect() {
		busy = true;
		error = null;
		await apiFetch(`/api/ui/bridge-credentials/${encodeURIComponent(bridge)}`, {
			method: 'DELETE'
		});
		busy = false;
		done = 'Disconnected.';
		await load();
	}

	onMount(() => void load());
</script>

<section class="flex max-w-md flex-col gap-3" data-part="page.login">
	<h2 class="text-base font-semibold">{text(node.title) ?? 'Your login'}</h2>
	{#if text(node.help)}<p class="text-sm text-muted-foreground">{text(node.help)}</p>{/if}
	{#if connected}
		<p class="text-sm">Connected as <strong>{name}</strong>.</p>
		<div>
			<Button size="sm" variant="outline" disabled={busy} onclick={disconnect}>Disconnect</Button>
		</div>
	{/if}
	<form class="flex flex-col gap-2" onsubmit={save}>
		<label class="text-sm" for="login-user">User name</label>
		<Input id="login-user" autocomplete="username" bind:value={username} required />
		<label class="text-sm" for="login-secret">Password</label>
		<Input
			id="login-secret"
			type="password"
			autocomplete="current-password"
			bind:value={secret}
			required
		/>
		<div>
			<Button size="sm" type="submit" disabled={busy}
				>{connected ? 'Replace login' : 'Connect'}</Button
			>
		</div>
	</form>
	{#if error}<p class="text-xs text-destructive" role="alert">{error}</p>{/if}
	{#if done}<p class="text-xs text-muted-foreground" role="status">{done}</p>{/if}
</section>

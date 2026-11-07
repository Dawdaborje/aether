<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';

	/**
	 * Asks before an action that changes or removes something. For one that cannot be undone,
	 * `typeToConfirm` makes the person type a word (an organization's name) first. `children`
	 * shows what the action will touch; `error` keeps the dialog open with the reason it failed.
	 */
	let {
		open = $bindable(false),
		title,
		description,
		confirmLabel = 'Confirm',
		destructive = false,
		typeToConfirm = null,
		busy = false,
		disabled = false,
		error = null,
		onConfirm,
		children
	}: {
		open?: boolean;
		title: string;
		description?: string;
		confirmLabel?: string;
		destructive?: boolean;
		typeToConfirm?: string | null;
		busy?: boolean;
		/** Not ready to confirm yet (still loading what the action will touch). */
		disabled?: boolean;
		error?: string | null;
		onConfirm: () => void | Promise<void>;
		children?: Snippet;
	} = $props();

	let typed = $state('');
	$effect(() => {
		if (open) typed = '';
	});

	const matches = $derived(!typeToConfirm || typed.trim() === typeToConfirm);
</script>

<AlertDialog.Root bind:open>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{title}</AlertDialog.Title>
			{#if description}
				<AlertDialog.Description>{description}</AlertDialog.Description>
			{/if}
		</AlertDialog.Header>
		{@render children?.()}
		{#if typeToConfirm}
			<label class="block space-y-2 text-sm">
				<span>
					This cannot be undone. Type <code class="font-mono font-semibold">{typeToConfirm}</code>
					to continue.
				</span>
				<Input bind:value={typed} autocomplete="off" spellcheck={false} disabled={busy} />
			</label>
		{/if}
		{#if error}
			<p class="text-sm text-destructive" role="alert">{error}</p>
		{/if}
		<AlertDialog.Footer>
			<AlertDialog.Cancel disabled={busy}>Cancel</AlertDialog.Cancel>
			<Button
				variant={destructive ? 'destructive' : 'default'}
				disabled={busy || disabled || !matches}
				onclick={() => void onConfirm()}
			>
				{busy ? 'Working…' : confirmLabel}
			</Button>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

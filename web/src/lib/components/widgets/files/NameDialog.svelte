<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';

	/** Asks for one line of text (a name). `onsubmit` answers an error message, or nothing when it worked. */
	let {
		open = $bindable(false),
		title,
		label = 'Name',
		initial = '',
		submitLabel = 'Save',
		onsubmit
	}: {
		open?: boolean;
		title: string;
		label?: string;
		initial?: string;
		submitLabel?: string;
		onsubmit: (value: string) => Promise<string | null | void>;
	} = $props();

	let value = $state('');
	let busy = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (open) {
			value = initial;
			error = null;
		}
	});

	async function submit() {
		if (!value.trim() || busy) return;
		busy = true;
		error = (await onsubmit(value.trim())) ?? null;
		busy = false;
		if (!error) open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header><Dialog.Title>{title}</Dialog.Title></Dialog.Header>
		<form
			class="space-y-3"
			onsubmit={(e) => {
				e.preventDefault();
				void submit();
			}}
		>
			<label class="block text-sm">
				<span class="mb-1 block text-muted-foreground">{label}</span>
				<!-- svelte-ignore a11y_autofocus -- the one field of the dialog -->
				<input
					bind:value
					autofocus
					class="h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
				/>
			</label>
			{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
			<Dialog.Footer>
				<Button type="button" variant="ghost" onclick={() => (open = false)}>Cancel</Button>
				<Button type="submit" disabled={busy || !value.trim()}>{submitLabel}</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>

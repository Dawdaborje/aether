<script lang="ts">
	import ConfirmDialog from './ConfirmDialog.svelte';
	import { confirmState } from './confirm.svelte';

	// Closing the dialog any way but the confirm button counts as "no".
	let open = $derived(confirmState.pending !== null);
	$effect(() => {
		if (!open && confirmState.pending) confirmState.answer(false);
	});
</script>

<ConfirmDialog
	bind:open
	title={confirmState.pending?.title ?? ''}
	description={confirmState.pending?.description}
	confirmLabel={confirmState.pending?.confirmLabel ?? 'Confirm'}
	destructive={confirmState.pending?.destructive ?? false}
	onConfirm={() => confirmState.answer(true)}
/>

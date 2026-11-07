<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Button } from '$lib/components/ui/button';

	let { node }: { node: PageNode } = $props();
	let open = $state(false);
</script>

<Button size="sm" variant="outline" onclick={() => (open = true)}>
	{String(node.title ?? 'Confirm')}
</Button>

<AlertDialog.Root bind:open>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{String(node.title ?? 'Confirm')}</AlertDialog.Title>
			<AlertDialog.Description>
				{String(node.description ?? 'Are you sure you want to continue?')}
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>{String(node.cancelLabel ?? 'Cancel')}</AlertDialog.Cancel>
			<AlertDialog.Action>{String(node.confirmLabel ?? 'Confirm')}</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

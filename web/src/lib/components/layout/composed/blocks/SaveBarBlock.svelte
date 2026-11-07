<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { pageState } from '../pageState.svelte';
</script>

<!-- Shows while the open form holds unsaved changes. -->
{#if pageState.form}
	<div data-part="shell.save-bar" class="flex items-center gap-2 px-1.5 text-sm" role="status">
		<span class="text-region-muted-foreground">Unsaved changes</span>
		<Button size="sm" variant="ghost" onclick={() => pageState.form?.discard()}>Discard</Button>
		<Button size="sm" disabled={pageState.form.saving} onclick={() => pageState.form?.save()}>
			{pageState.form.saving ? 'Saving…' : 'Save'}
		</Button>
	</div>
{/if}

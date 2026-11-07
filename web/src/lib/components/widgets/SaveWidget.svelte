<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import { getFormState, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const form = getFormState();
	// A `label` left as `{{…}}` (a component prop nobody gave) is no label.
	const given = $derived.by(() => {
		const label = text(node.label);
		return label && !/^\{\{.*\}\}$/.test(label) ? label : undefined;
	});
	const label = $derived(given ?? form?.saveLabel ?? 'Save');
</script>

<!-- Saves the form it is in; place it in the form's bar (`<save/>`, or `base_ui.form_bar`). -->
{#if form}
	<Button
		type="button"
		size="sm"
		disabled={!form.save || form.saving}
		onclick={() => void form.save?.()}
	>
		{form.saving ? 'Saving…' : label}
	</Button>
{/if}

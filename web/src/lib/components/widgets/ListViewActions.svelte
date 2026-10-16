<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';

	let {
		actions,
		row,
		onRun
	}: {
		actions: PageNode[];
		row: Record<string, unknown>;
		onRun: (action: PageNode, row: Record<string, unknown>) => void;
	} = $props();

	/**
	 * A condition on the row: `field` is true when the row's field is set (not false, empty or
	 * missing), `!field` when it is not. Several, separated by commas, are an OR.
	 * Used by `<action disabled_when="is_default">` and `hidden_when`.
	 */
	function holds(rule: unknown): boolean {
		if (typeof rule !== 'string' || !rule.trim()) return false;
		return rule.split(',').some((part) => {
			const term = part.trim();
			if (!term) return false;
			const negate = term.startsWith('!');
			const field = negate ? term.slice(1).trim() : term;
			const value = row[field];
			const set = value !== undefined && value !== null && value !== false && value !== '' && value !== 0;
			return negate ? !set : set;
		});
	}

	const rule = (action: PageNode, snake: string, camel: string) =>
		(action as Record<string, unknown>)[snake] ?? (action as Record<string, unknown>)[camel];

	const visible = $derived(
		actions.filter((action) => !holds(rule(action, 'hidden_when', 'hiddenWhen')))
	);
</script>

<div class="flex justify-end gap-1">
	{#each visible as action (action.name ?? action.function)}
		<Button
			size="sm"
			variant={action.danger ? 'destructive' : 'outline'}
			disabled={holds(rule(action, 'disabled_when', 'disabledWhen'))}
			onclick={(event: MouseEvent) => {
				event.stopPropagation();
				onRun(action, row);
			}}
		>
			{String(action.label ?? action.name ?? 'Action')}
		</Button>
	{/each}
</div>

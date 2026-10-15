<script lang="ts">
	import type { OrgRole } from '$lib/org/api';
	import { kindLabel, resolveRoleDisplay, type RoleKind } from './roleDisplay';

	let {
		name,
		catalog = null,
		label: labelOverride = undefined,
		kind: kindOverride = undefined,
		removable = false,
		disabled = false,
		onRevoke
	}: {
		name: string;
		catalog?: OrgRole[] | null;
		label?: string;
		kind?: RoleKind;
		removable?: boolean;
		disabled?: boolean;
		onRevoke?: () => void;
	} = $props();

	const display = $derived(resolveRoleDisplay(name, catalog));
	const label = $derived(labelOverride ?? display.label);
	const kind = $derived(kindOverride ?? display.kind);
	const title = $derived(`${label} (${name}) · ${kindLabel(kind)}`);
</script>

<span
	class="inline-flex max-w-full items-center gap-1 rounded-sm border border-border/80 bg-muted/60 px-2 py-0.5 text-sm"
	{title}
>
	<span class="truncate font-medium">{label}</span>
	{#if kind === 'custom'}
		<span class="shrink-0 text-xs uppercase tracking-wide text-muted-foreground">Custom</span>
	{:else if kind === 'system'}
		<span class="shrink-0 text-xs uppercase tracking-wide text-muted-foreground">System</span>
	{/if}
	{#if removable && onRevoke}
		<button
			type="button"
			class="shrink-0 text-muted-foreground hover:text-destructive disabled:opacity-50"
			title={`Revoke ${label}`}
			aria-label={`Revoke ${label}`}
			{disabled}
			onclick={(event) => {
				event.stopPropagation();
				onRevoke();
			}}
		>
			×
		</button>
	{/if}
</span>

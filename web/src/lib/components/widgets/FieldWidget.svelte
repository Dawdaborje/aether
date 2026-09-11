<script lang="ts">
	import type { FieldType, PageNode } from '$lib/dsl/types';
	import { Input } from '$lib/components/ui/input';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Label } from '$lib/components/ui/label';
	import { Badge } from '$lib/components/ui/badge';
	import { NativeSelect } from '$lib/components/ui/native-select';

	let { node }: { node: PageNode } = $props();

	const fieldType = $derived(
		(typeof node.fieldType === 'string' ? node.fieldType : 'char') as FieldType
	);
	const label = $derived(String(node.label ?? node.name ?? 'Field'));
	const options = $derived(Array.isArray(node.options) ? (node.options as string[]) : []);
	let value = $state('');
</script>

<div class="space-y-1.5">
	{#if node.type !== 'badge' && node.type !== 'avatar'}
		<Label class="text-xs font-medium text-muted-foreground">{label}</Label>
	{/if}

	{#if fieldType === 'text'}
		<Textarea rows={3} placeholder={label} bind:value />
	{:else if fieldType === 'boolean'}
		<label class="flex items-center gap-2 text-sm">
			<input type="checkbox" class="size-4 rounded border-input" />
			{label}
		</label>
	{:else if fieldType === 'selection'}
		<NativeSelect>
			<option value="">Select…</option>
			{#each options as opt}
				<option value={opt}>{opt}</option>
			{/each}
		</NativeSelect>
	{:else if fieldType === 'badge'}
		<Badge>{label}</Badge>
	{:else if fieldType === 'progress'}
		<div class="h-2 w-full overflow-hidden rounded-full bg-muted">
			<div class="h-full w-2/3 bg-primary"></div>
		</div>
	{:else}
		<Input
			type={fieldType === 'integer' || fieldType === 'float' || fieldType === 'currency'
				? 'number'
				: fieldType === 'date'
					? 'date'
					: fieldType === 'datetime'
						? 'datetime-local'
						: 'text'}
			placeholder={label}
			bind:value
		/>
	{/if}
</div>

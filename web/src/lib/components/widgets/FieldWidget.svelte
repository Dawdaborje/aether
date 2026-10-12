<script lang="ts">
	import type { FieldType, PageNode } from '$lib/dsl/types';
	import { Input } from '$lib/components/ui/input';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Label } from '$lib/components/ui/label';
	import { Badge } from '$lib/components/ui/badge';
	import { NativeSelect } from '$lib/components/ui/native-select';
	import { getFormState } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const fieldType = $derived(
		(typeof node.fieldType === 'string' ? node.fieldType : 'char') as FieldType
	);
	const name = $derived(typeof node.name === 'string' ? node.name : '');
	const label = $derived(String(node.label ?? node.name ?? 'Field'));
	const required = $derived(node.required === true);
	const options = $derived(
		Array.isArray(node.options)
			? (node.options as string[])
			: typeof node.options === 'string'
				? node.options.split(',').map((option) => option.trim()).filter(Boolean)
				: []
	);

	// Inside a form the field edits the form's values; elsewhere it only holds its own.
	const form = getFormState();
	let own = $state<unknown>('');
	const id = $props.id();

	const value = {
		get current(): unknown {
			return form && name ? (form.values[name] ?? '') : own;
		},
		set current(next: unknown) {
			if (form && name) form.values[name] = next;
			else own = next;
		}
	};
</script>

<div class="space-y-1.5">
	{#if node.type !== 'badge' && node.type !== 'avatar' && fieldType !== 'boolean'}
		<Label for={id} class="text-xs font-medium text-muted-foreground">
			{label}{#if required}<span class="text-destructive"> *</span>{/if}
		</Label>
	{/if}

	{#if fieldType === 'text'}
		<Textarea
			{id}
			rows={3}
			{required}
			placeholder={label}
			value={String(value.current)}
			oninput={(event) => (value.current = event.currentTarget.value)}
		/>
	{:else if fieldType === 'boolean'}
		<label class="flex items-center gap-2 text-sm">
			<input
				type="checkbox"
				class="size-4 rounded border-input"
				checked={value.current === true}
				onchange={(event) => (value.current = event.currentTarget.checked)}
			/>
			{label}
		</label>
	{:else if fieldType === 'selection'}
		<NativeSelect
			{id}
			{required}
			value={String(value.current)}
			onchange={(event) => (value.current = event.currentTarget.value)}
		>
			<option value="">Select…</option>
			{#each options as option (option)}
				<option value={option}>{option}</option>
			{/each}
		</NativeSelect>
	{:else if fieldType === 'badge' || node.type === 'badge'}
		<Badge>{String(value.current || label)}</Badge>
	{:else if fieldType === 'avatar' || node.type === 'avatar'}
		{@const text = String(value.current || label || '?')}
		{@const initials = text
			.trim()
			.split(/\s+/)
			.slice(0, 2)
			.map((p) => p[0]?.toUpperCase() ?? '')
			.join('') || '?'}
		<span
			class="inline-flex size-8 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
			title={text}
		>
			{initials}
		</span>
	{:else if fieldType === 'progress'}
		{@const raw = value.current}
		{@const pct = Math.max(
			0,
			Math.min(100, typeof raw === 'number' ? raw : Number(raw) || 0)
		)}
		<div class="h-2 w-full overflow-hidden rounded-full bg-muted" title="{pct}%">
			<div class="h-full bg-primary transition-[width]" style="width: {pct}%"></div>
		</div>
	{:else}
		{@const numeric = fieldType === 'integer' || fieldType === 'float' || fieldType === 'currency'}
		<Input
			{id}
			{required}
			type={numeric
				? 'number'
				: fieldType === 'date'
					? 'date'
					: fieldType === 'datetime'
						? 'datetime-local'
						: 'text'}
			step={fieldType === 'float' || fieldType === 'currency' ? 'any' : undefined}
			placeholder={label}
			value={String(value.current)}
			oninput={(event) => {
				const raw = event.currentTarget.value;
				value.current = numeric ? (raw === '' ? '' : Number(raw)) : raw;
			}}
		/>
	{/if}
</div>

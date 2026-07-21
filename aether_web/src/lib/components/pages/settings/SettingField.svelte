<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Badge } from '$lib/components/ui/badge';
	import { updateSetting, type CatalogItem } from '$lib/settings/api';

	let {
		item,
		onsaved
	}: {
		item: CatalogItem;
		onsaved?: (item: CatalogItem) => void;
	} = $props();

	let draft = $state(cloneValue(item.value));
	let saving = $state(false);
	let error = $state('');
	let savedFlash = $state(false);

	$effect(() => {
		draft = cloneValue(item.value);
	});

	function cloneValue(value: unknown): unknown {
		if (Array.isArray(value)) return [...value];
		if (value && typeof value === 'object') return structuredClone(value);
		return value;
	}

	function listAsText(value: unknown): string {
		if (Array.isArray(value)) return value.map(String).join(', ');
		return String(value ?? '');
	}

	function parseList(text: string): string[] {
		return text
			.split(',')
			.map((s) => s.trim())
			.filter(Boolean);
	}

	async function save() {
		error = '';
		saving = true;
		try {
			let value = draft;
			if (item.value_type === 'list' && typeof draft === 'string') {
				value = parseList(draft);
			}
			if (item.value_type === 'json' && typeof draft === 'string') {
				value = JSON.parse(draft);
			}
			if (item.value_type === 'number' && typeof draft === 'string') {
				value = Number(draft);
			}
			const updated = await updateSetting(item.key, value);
			onsaved?.({ ...item, ...updated, label: item.label });
			savedFlash = true;
			setTimeout(() => (savedFlash = false), 1500);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Save failed';
		} finally {
			saving = false;
		}
	}
</script>

<div class="space-y-3 border-b border-border py-6 last:border-b-0">
	<div class="flex flex-wrap items-start justify-between gap-3">
		<div class="min-w-0 space-y-1">
			<div class="flex flex-wrap items-center gap-2">
				<h3 class="text-sm font-semibold tracking-tight">{item.label}</h3>
				<Badge variant="outline" class="text-[10px] uppercase tracking-wider">
					{item.source}
				</Badge>
			</div>
			<p class="font-mono text-[11px] text-muted-foreground">{item.key}</p>
			{#if item.description}
				<p class="text-sm text-muted-foreground">{item.description}</p>
			{/if}
		</div>
		<Button size="sm" variant="outline" disabled={saving} onclick={save}>
			{saving ? 'Saving…' : savedFlash ? 'Saved' : 'Save'}
		</Button>
	</div>

	{#if item.value_type === 'boolean'}
		<label class="flex items-center gap-3 text-sm">
			<input
				type="checkbox"
				class="size-4 accent-primary"
				checked={Boolean(draft)}
				onchange={(e) => (draft = e.currentTarget.checked)}
			/>
			<span>{draft ? 'Enabled' : 'Disabled'}</span>
		</label>
	{:else if item.value_type === 'list'}
		<div class="space-y-2">
			<Label for={item.key}>Comma-separated values</Label>
			<Input
				id={item.key}
				value={typeof draft === 'string' ? draft : listAsText(draft)}
				oninput={(e) => (draft = e.currentTarget.value)}
			/>
		</div>
	{:else if item.value_type === 'json'}
		<div class="space-y-2">
			<Label for={item.key}>JSON</Label>
			<Textarea
				id={item.key}
				rows={5}
				value={typeof draft === 'string' ? draft : JSON.stringify(draft, null, 2)}
				oninput={(e) => (draft = e.currentTarget.value)}
			/>
		</div>
	{:else}
		<div class="space-y-2">
			<Label for={item.key}>Value</Label>
			<Input
				id={item.key}
				type={item.value_type === 'number' ? 'number' : 'text'}
				value={draft == null ? '' : String(draft)}
				oninput={(e) => (draft = e.currentTarget.value)}
			/>
		</div>
	{/if}

	{#if item.long_description}
		<p class="text-xs text-muted-foreground">{item.long_description}</p>
	{/if}
	{#if error}
		<p class="text-sm text-destructive">{error}</p>
	{/if}
</div>

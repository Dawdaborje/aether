<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Textarea } from '$lib/components/ui/textarea';
	import Switch from '$lib/components/settings/Switch.svelte';
	import { updateSetting, type CatalogItem } from '$lib/settings/api';

	let {
		item,
		onsaved
	}: {
		item: CatalogItem;
		onsaved?: (item: CatalogItem) => void;
	} = $props();

	const isBoolean = $derived(item.value_type === 'boolean');
	/** An API key or password: never shown, only replaced or cleared. */
	const isSecret = $derived(item.value_type === 'secret');

	/**
	 * What the control shows. It starts as the saved value, follows it when it
	 * changes (after a save, or on reload), and can be edited in between.
	 */
	let draft = $derived<string | boolean>(toDraft(item.value));
	let saving = $state(false);
	let error = $state('');
	let justSaved = $state(false);

	function toDraft(value: unknown): string | boolean {
		if (item.value_type === 'boolean') return Boolean(value);
		if (item.value_type === 'list') return Array.isArray(value) ? value.map(String).join(', ') : String(value ?? '');
		if (item.value_type === 'json') return JSON.stringify(value ?? null, null, 2);
		return value == null ? '' : String(value);
	}

	/** The typed value the draft stands for; throws a readable error when it is invalid. */
	function parse(value: string | boolean): unknown {
		if (typeof value === 'boolean') return value;
		if (item.value_type === 'list') {
			return value
				.split(',')
				.map((part) => part.trim())
				.filter(Boolean);
		}
		if (item.value_type === 'json') {
			try {
				return JSON.parse(value);
			} catch {
				throw new Error('This is not valid JSON.');
			}
		}
		if (item.value_type === 'number') {
			const number = Number(value);
			if (value.trim() === '' || Number.isNaN(number)) throw new Error('Enter a number.');
			return number;
		}
		return value;
	}

	const problem = $derived.by(() => {
		try {
			parse(draft);
			return '';
		} catch (err) {
			return err instanceof Error ? err.message : 'Invalid value.';
		}
	});
	const dirty = $derived(!isBoolean && JSON.stringify(draft) !== JSON.stringify(toDraft(item.value)));
	const listPreview = $derived(
		item.value_type === 'list' && typeof draft === 'string' && !problem
			? (parse(draft) as string[])
			: []
	);

	async function save(next: string | boolean = draft) {
		error = '';
		saving = true;
		try {
			const updated = await updateSetting(item.key, parse(next));
			onsaved?.({ ...item, ...updated, label: item.label, value_type: item.value_type });
			justSaved = true;
			setTimeout(() => (justSaved = false), 2000);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Save failed';
			draft = toDraft(item.value);
		} finally {
			saving = false;
		}
	}

	function toggle(checked: boolean) {
		draft = checked;
		void save(checked);
	}
</script>

<div class="grid gap-4 p-5 md:grid-cols-[minmax(0,1fr)_minmax(0,24rem)] md:gap-8">
	<div class="min-w-0 space-y-1.5">
		<div class="flex flex-wrap items-center gap-2">
			<h3 class="text-sm font-semibold tracking-tight">{item.label}</h3>
			<span
				class="rounded-sm px-1.5 py-0.5 text-[10px] font-semibold tracking-wider uppercase {item.source ===
				'org'
					? 'bg-primary/10 text-primary'
					: 'bg-muted text-muted-foreground'}"
			>
				{item.source === 'org' ? 'Organization' : 'Default'}
			</span>
		</div>
		{#if item.description}
			<p class="text-sm text-muted-foreground">{item.description}</p>
		{/if}
		{#if item.long_description}
			<p class="text-xs text-muted-foreground">{item.long_description}</p>
		{/if}
		<p class="font-mono text-[11px] text-muted-foreground/70">{item.key}</p>
	</div>

	<form
		class="space-y-2"
		onsubmit={(event) => {
			event.preventDefault();
			if (dirty && !problem) void save();
		}}
	>
		{#if isBoolean}
			<div class="flex items-center gap-3">
				<Switch
					checked={Boolean(draft)}
					disabled={saving}
					label={item.label}
					onchange={toggle}
				/>
				<span class="text-sm text-muted-foreground">{draft ? 'Enabled' : 'Disabled'}</span>
			</div>
		{:else if item.value_type === 'json'}
			<Textarea
				rows={6}
				class="rounded-sm border border-input bg-card px-3 py-2 font-mono text-xs shadow-xs focus-visible:border-ring"
				aria-label={item.label}
				value={String(draft)}
				oninput={(event) => (draft = event.currentTarget.value)}
			/>
		{:else if isSecret}
			<Input
				class="rounded-sm border-input bg-card px-3 shadow-xs focus-visible:border-ring"
				aria-label={item.label}
				type="password"
				autocomplete="new-password"
				placeholder={item.has_value ? 'Saved. Type a new value to replace it' : 'Not set'}
				value={String(draft)}
				oninput={(event) => (draft = event.currentTarget.value)}
			/>
			<p class="text-xs text-muted-foreground">
				{item.has_value ? 'A value is saved and stays hidden.' : 'No value is saved.'}
			</p>
		{:else}
			<Input
				class="rounded-sm border-input bg-card px-3 shadow-xs focus-visible:border-ring"
				aria-label={item.label}
				type={item.value_type === 'number' ? 'number' : 'text'}
				placeholder={item.value_type === 'list' ? 'one, two, three' : ''}
				value={String(draft)}
				oninput={(event) => (draft = event.currentTarget.value)}
			/>
			{#if listPreview.length > 0}
				<ul class="flex flex-wrap gap-1.5" aria-label="Values">
					{#each listPreview as value (value)}
						<li class="rounded-sm bg-muted px-2 py-0.5 text-xs">{value}</li>
					{/each}
				</ul>
			{/if}
		{/if}

		{#if problem && dirty}
			<p class="text-xs text-destructive" role="alert">{problem}</p>
		{/if}
		{#if error}
			<p class="text-sm text-destructive" role="alert">{error}</p>
		{/if}

		<div class="flex h-8 items-center gap-2">
			{#if dirty}
				<Button type="submit" size="sm" disabled={saving || !!problem}>
					{saving ? 'Saving…' : 'Save changes'}
				</Button>
				<Button
					type="button"
					size="sm"
					variant="ghost"
					disabled={saving}
					onclick={() => (draft = toDraft(item.value))}>Discard</Button
				>
			{:else if isSecret && item.has_value}
				<Button
					type="button"
					size="sm"
					variant="ghost"
					disabled={saving}
					onclick={() => void save('')}
				>
					{item.source === 'org' ? 'Remove (use the default)' : 'Remove'}
				</Button>
				{#if justSaved}
					<span class="flex items-center gap-1 text-xs font-medium text-primary">
						<CheckIcon class="size-3.5" /> Saved
					</span>
				{/if}
			{:else if justSaved}
				<span class="flex items-center gap-1 text-xs font-medium text-primary">
					<CheckIcon class="size-3.5" /> Saved
				</span>
			{/if}
		</div>
	</form>
</div>

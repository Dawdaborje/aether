<script lang="ts">
	import type { PageNode } from '$lib/dsl/types';
	import { cn } from '$lib/utils';
	import { getFormState, getPageState, rowsOf, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const form = getFormState();

	/// `field`: the record field that holds the state. The steps come from, in order:
	/// `options="draft, confirmed"`, `source="plugin.function"` (rows; `optionLabel` names the
	/// label field, `sourceparam` the record field whose value is sent as `id`), or the model
	/// field's own selection options. `function`: called when a step is clicked, with the route
	/// values, the record's `id`, and the chosen step as `status` and as `<field>`; with
	/// `payload="record"` the whole record is sent too.
	/// A prop the component's user did not give stays written as `{{name}}`: that is no value.
	const prop = (value: unknown): string | undefined => {
		const given = text(value);
		return given && !/^\{\{.*\}\}$/.test(given) ? given : undefined;
	};
	const field = $derived(prop(node.field) ?? 'status');
	const fn = $derived(prop(node.function));
	const source = $derived(prop(node.source));
	const sourceParam = $derived(prop(node.sourceparam));
	const optionLabel = $derived(prop(node.optionLabel) ?? 'name');

	interface Step {
		value: string;
		label: string;
	}
	const titled = (state: string) =>
		state.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase());

	const staticSteps = $derived.by((): Step[] => {
		const listed = (prop(node.options) ?? prop(node.states) ?? '')
			.split(',')
			.map((state) => state.trim())
			.filter(Boolean);
		if (listed.length > 0) return listed.map((value) => ({ value, label: titled(value) }));
		const hinted = node.modelOptions;
		if (Array.isArray(hinted)) {
			return hinted
				.filter((o): o is { value: string; label?: string } => !!o && typeof o === 'object')
				.map((o) => ({ value: String(o.value), label: o.label ? String(o.label) : titled(String(o.value)) }));
		}
		return [];
	});

	let loaded = $state<Step[]>([]);
	const scope = $derived(sourceParam ? String(form?.values[sourceParam] ?? '') : '');
	$effect(() => {
		void page?.tick;
		if (!source || !page) return;
		if (sourceParam && !scope) {
			loaded = [];
			return;
		}
		let current = true;
		void page.call(source, sourceParam ? { ...page.params, id: scope } : { ...page.params }).then((result) => {
			if (!current || !result.ok) return;
			loaded = rowsOf(result.data).map((row) => ({
				value: String(row.id ?? ''),
				label: String(row[optionLabel] ?? row.display_name ?? row.id ?? '')
			}));
		});
		return () => {
			current = false;
		};
	});

	const steps = $derived(source ? loaded : staticSteps);
	// Ids may be written with or without the table prefix.
	const same = (a: string, b: string) => a === b || a.split(':').pop() === b.split(':').pop();
	const current = $derived(String(form?.values[field] ?? text(node.value) ?? ''));
	const index = $derived(steps.findIndex((step) => same(step.value, current)));

	let busy = $state(false);

	async function choose(state: string) {
		if (!fn || !page || busy || same(state, current)) return;
		busy = true;
		const id = form?.values.id;
		// `payload="record"` sends the whole record, for a save function that wants every field.
		const whole = prop(node.payload) === 'record' ? { ...(form?.values ?? {}) } : {};
		const payload: Record<string, unknown> = {
			...page.params,
			...whole,
			...(id !== undefined ? { id: String(id) } : {}),
			status: state,
			[field]: state
		};
		const result = await page.call(fn, payload);
		busy = false;
		if (result.ok) page.refresh();
	}
</script>

{#if steps.length > 0}
	<ol
		class="flex flex-wrap items-center overflow-hidden rounded-md border border-border text-xs"
		data-part="form.statusbar"
		aria-label={titled(field)}
	>
		{#each steps as step, i (step.value)}
			{@const active = i === index}
			{@const done = index >= 0 && i < index}
			<li class={cn(i > 0 && 'border-l border-border')}>
				<button
					type="button"
					disabled={!fn || busy}
					aria-current={active ? 'step' : undefined}
					title={fn && !active ? `Set to ${step.label}` : undefined}
					class={cn(
						'px-3 py-1.5 font-medium transition-colors',
						active
							? 'bg-primary text-primary-foreground'
							: done
								? 'bg-muted text-foreground'
								: 'bg-card text-muted-foreground',
						fn && !active && 'hover:bg-accent hover:text-accent-foreground',
						(!fn || active) && 'cursor-default'
					)}
					onclick={() => choose(step.value)}
				>
					{step.label}
				</button>
			</li>
		{/each}
	</ol>
{/if}

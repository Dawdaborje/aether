<script lang="ts">
	import type { BoardFilter } from './types';

	let {
		filters,
		values,
		onchange
	}: {
		filters: BoardFilter[];
		values: Record<string, unknown>;
		onchange: (name: string, value: unknown, immediate: boolean) => void;
	} = $props();

	const PERIODS: [string, string][] = [
		['', 'Any time'],
		['today', 'Today'],
		['this_week', 'This week'],
		['this_month', 'This month'],
		['this_quarter', 'This quarter'],
		['this_year', 'This year'],
		['last_7_days', 'Last 7 days'],
		['last_30_days', 'Last 30 days'],
		['last_90_days', 'Last 90 days'],
		['last_month', 'Last month'],
		['last_quarter', 'Last quarter'],
		['last_year', 'Last year'],
		['custom', 'Custom range…']
	];

	const control =
		'h-9 rounded-md border border-input bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring';

	function periodChoice(value: unknown): string {
		if (value && typeof value === 'object') return 'custom';
		return typeof value === 'string' && value !== 'none' ? value : '';
	}
	function range(value: unknown): { from: string; to: string } {
		const object = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>;
		return { from: String(object.from ?? ''), to: String(object.to ?? '') };
	}
	function chosen(value: unknown): unknown[] {
		return Array.isArray(value) ? value : [];
	}
	function same(a: unknown, b: unknown): boolean {
		return JSON.stringify(a) === JSON.stringify(b);
	}
	function toggle(filter: BoardFilter, option: unknown) {
		const current = chosen(values[filter.name]);
		const next = current.some((v) => same(v, option))
			? current.filter((v) => !same(v, option))
			: [...current, option];
		onchange(filter.name, next, true);
	}
</script>

{#if filters.length > 0}
	<div
		class="flex flex-wrap items-end gap-x-4 gap-y-2 rounded-lg border border-border bg-card px-3 py-2"
		data-part="board.filters"
	>
		{#each filters as filter (filter.name)}
			<div class="flex flex-col gap-1">
				<label
					class="text-[11px] font-medium tracking-wide text-muted-foreground uppercase"
					for="board-{filter.name}"
				>
					{filter.label}
				</label>
				{#if filter.kind === 'period'}
					{@const choice = periodChoice(values[filter.name])}
					<div class="flex items-center gap-2">
						<select
							id="board-{filter.name}"
							class={control}
							value={choice}
							onchange={(e) => {
								const next = e.currentTarget.value;
								onchange(
									filter.name,
									next === 'custom' ? { from: '', to: '' } : next,
									next !== 'custom'
								);
							}}
						>
							{#each PERIODS as [value, label] (value)}
								<option {value}>{label}</option>
							{/each}
						</select>
						{#if choice === 'custom'}
							{@const r = range(values[filter.name])}
							<input
								type="date"
								class={control}
								aria-label="{filter.label} from"
								value={r.from}
								onchange={(e) => onchange(filter.name, { ...r, from: e.currentTarget.value }, true)}
							/>
							<span class="text-xs text-muted-foreground">to</span>
							<input
								type="date"
								class={control}
								aria-label="{filter.label} to"
								value={r.to}
								onchange={(e) => onchange(filter.name, { ...r, to: e.currentTarget.value }, true)}
							/>
						{/if}
					</div>
				{:else if filter.kind === 'select'}
					<div
						class="flex max-w-xl flex-wrap gap-1"
						id="board-{filter.name}"
						role="group"
						aria-label={filter.label}
					>
						{#each filter.options ?? [] as option (JSON.stringify(option.value))}
							{@const on = chosen(values[filter.name]).some((v) => same(v, option.value))}
							<button
								type="button"
								aria-pressed={on}
								class="h-8 rounded-full border px-3 text-xs transition-colors {on
									? 'border-primary bg-primary text-primary-foreground'
									: 'border-input bg-background hover:bg-muted'}"
								onclick={() => toggle(filter, option.value)}
							>
								{option.label}
							</button>
						{:else}
							<span class="text-xs text-muted-foreground">No values</span>
						{/each}
					</div>
				{:else}
					<input
						id="board-{filter.name}"
						type="search"
						class="{control} w-44"
						placeholder="Contains…"
						value={typeof values[filter.name] === 'string' ? values[filter.name] : ''}
						oninput={(e) => onchange(filter.name, e.currentTarget.value, false)}
					/>
				{/if}
			</div>
		{/each}
	</div>
{/if}

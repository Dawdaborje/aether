<script lang="ts">
	import { goto } from '$app/navigation';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import type { PageNode } from '$lib/dsl/types';
	import PageRenderer from './PageRenderer.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageState, interpolate, keyOf, rowsOf, text } from '$lib/pages/pageContext.svelte';

	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const source = $derived(text(node.source));
	const openTemplate = $derived(text(node.open));
	const dateStart = $derived(text(node.date_start) ?? text(node.dateStart) ?? 'starts_at');
	const dateStop = $derived(text(node.date_stop) ?? text(node.dateStop) ?? '');
	const colorField = $derived(text(node.color) ?? '');
	const titleField = $derived.by(() => {
		const field = (node.children ?? []).find((c) => c.type === 'field');
		return text(field?.name) ?? text(field?.field) ?? 'title';
	});
	const empty = $derived((node.children ?? []).find((c) => c.type === 'empty'));

	const now = new Date();
	let cursor = $state(new Date(now.getFullYear(), now.getMonth(), 1));
	let rows = $state<Record<string, unknown>[] | null>(null);
	let failure = $state<string | null>(null);
	let latest = 0;

	const monthLabel = $derived(
		cursor.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
	);

	function startOfMonth(date: Date): Date {
		return new Date(date.getFullYear(), date.getMonth(), 1);
	}

	function endOfMonth(date: Date): Date {
		return new Date(date.getFullYear(), date.getMonth() + 1, 0, 23, 59, 59, 999);
	}

	/** Grid starts on Sunday of the week that contains the first of the month. */
	function gridStart(date: Date): Date {
		const first = startOfMonth(date);
		const day = first.getDay();
		const start = new Date(first);
		start.setDate(first.getDate() - day);
		start.setHours(0, 0, 0, 0);
		return start;
	}

	function gridEnd(date: Date): Date {
		const last = endOfMonth(date);
		const day = last.getDay();
		const end = new Date(last);
		end.setDate(last.getDate() + (6 - day));
		end.setHours(23, 59, 59, 999);
		return end;
	}

	function toIso(date: Date): string {
		return date.toISOString();
	}

	function dayKey(date: Date): string {
		return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
	}

	function parseWhen(value: unknown): Date | null {
		if (value === null || value === undefined || value === '') return null;
		const date = new Date(String(value));
		return Number.isNaN(date.getTime()) ? null : date;
	}

	async function load() {
		const current = ++latest;
		if (!source || !page) {
			rows = [];
			return;
		}
		const start = gridStart(cursor);
		const end = gridEnd(cursor);
		const result = await page.call(source, {
			...page.params,
			start: toIso(start),
			end: toIso(end),
			scale: 'month'
		});
		if (current !== latest) return;
		if (result.ok) {
			rows = rowsOf(result.data);
			failure = null;
		} else {
			rows = rows ?? [];
			failure = result.error;
		}
	}

	$effect(() => {
		void page?.plugin;
		void page?.tick;
		void source;
		void cursor;
		void load();
	});

	type DayCell = { date: Date; inMonth: boolean; events: Record<string, unknown>[] };

	const days = $derived.by((): DayCell[] => {
		const start = gridStart(cursor);
		const month = cursor.getMonth();
		const byDay = new Map<string, Record<string, unknown>[]>();
		for (const row of rows ?? []) {
			const begin = parseWhen(row[dateStart]);
			if (!begin) continue;
			const stop = dateStop ? parseWhen(row[dateStop]) : begin;
			const end = stop && stop >= begin ? stop : begin;
			const walk = new Date(begin.getFullYear(), begin.getMonth(), begin.getDate());
			const last = new Date(end.getFullYear(), end.getMonth(), end.getDate());
			while (walk <= last) {
				const key = dayKey(walk);
				const list = byDay.get(key) ?? [];
				list.push(row);
				byDay.set(key, list);
				walk.setDate(walk.getDate() + 1);
			}
		}
		const cells: DayCell[] = [];
		for (let i = 0; i < 42; i++) {
			const date = new Date(start);
			date.setDate(start.getDate() + i);
			cells.push({
				date,
				inMonth: date.getMonth() === month,
				events: byDay.get(dayKey(date)) ?? []
			});
		}
		return cells;
	});

	function openEvent(row: Record<string, unknown>) {
		if (openTemplate) void goto(navHref(interpolate(openTemplate, row)));
	}

	function eventTitle(row: Record<string, unknown>): string {
		const value = row[titleField];
		return value === null || value === undefined || value === '' ? keyOf(row.id) || 'Event' : String(value);
	}

	const weekdayLabels = $derived.by(() => {
		const base = gridStart(cursor);
		return Array.from({ length: 7 }, (_, i) => {
			const d = new Date(base);
			d.setDate(base.getDate() + i);
			return d.toLocaleDateString(undefined, { weekday: 'short' });
		});
	});
</script>

<div class="space-y-3">
	<div class="flex items-center justify-between gap-3">
		<h2 class="text-lg font-medium tracking-tight">{monthLabel}</h2>
		<div class="flex items-center gap-1">
			<Button
				size="sm"
				variant="outline"
				aria-label="Previous month"
				onclick={() => {
					cursor = new Date(cursor.getFullYear(), cursor.getMonth() - 1, 1);
				}}
			>
				<ChevronLeftIcon class="size-4" />
			</Button>
			<Button
				size="sm"
				variant="outline"
				onclick={() => {
					const now = new Date();
					cursor = new Date(now.getFullYear(), now.getMonth(), 1);
				}}
			>
				Today
			</Button>
			<Button
				size="sm"
				variant="outline"
				aria-label="Next month"
				onclick={() => {
					cursor = new Date(cursor.getFullYear(), cursor.getMonth() + 1, 1);
				}}
			>
				<ChevronRightIcon class="size-4" />
			</Button>
		</div>
	</div>

	{#if failure}
		<p class="text-sm text-destructive" role="alert">{failure}</p>
	{/if}

	{#if rows === null}
		<div class="space-y-2" aria-busy="true">
			<Skeleton class="h-10 w-full" />
			<Skeleton class="h-64 w-full" />
		</div>
	{:else if rows.length === 0 && empty}
		<PageRenderer node={empty} />
	{:else}
		<div class="overflow-hidden rounded-md border border-border">
			<div class="grid grid-cols-7 border-b border-border bg-muted/40 text-center text-xs font-medium text-muted-foreground">
				{#each weekdayLabels as label (label)}
					<div class="px-1 py-2">{label}</div>
				{/each}
			</div>
			<div class="grid grid-cols-7">
				{#each days as day (dayKey(day.date))}
					<div
						class="min-h-24 border-b border-r border-border p-1 last:border-r-0 {day.inMonth
							? 'bg-background'
							: 'bg-muted/20 text-muted-foreground'}"
					>
						<div class="px-1 text-xs tabular-nums">{day.date.getDate()}</div>
						<ul class="mt-1 space-y-0.5">
							{#each day.events.slice(0, 3) as event (String(event.id ?? eventTitle(event)))}
								<li>
									<button
										type="button"
										class="w-full truncate rounded px-1 py-0.5 text-left text-xs {openTemplate
											? 'hover:bg-accent'
											: ''} {colorField && event[colorField]
											? 'bg-primary/10 text-primary'
											: 'bg-muted text-foreground'}"
										onclick={() => openEvent(event)}
									>
										{eventTitle(event)}
									</button>
								</li>
							{/each}
							{#if day.events.length > 3}
								<li class="px-1 text-[10px] text-muted-foreground">+{day.events.length - 3} more</li>
							{/if}
						</ul>
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>

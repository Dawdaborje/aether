<script lang="ts">
	import { tick } from 'svelte';
	import CalendarIcon from '@lucide/svelte/icons/calendar';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import XIcon from '@lucide/svelte/icons/x';
	import * as Popover from '$lib/components/ui/popover';
	import { Button } from '$lib/components/ui/button';
	import { cn } from '$lib/utils';
	import {
		addDays,
		addMonths,
		compare,
		formatDate,
		formatDateTime,
		localeFirstDay,
		monthGrid,
		parseTyped,
		parseValue,
		sameDay,
		type Ymd
	} from './calendar-math';

	/**
	 * A calendar for picking a date (or a date and a time).
	 *
	 * - Type a date, or open the calendar. Click the month name to pick a month and the year to
	 *   jump through years twelve at a time, so a birth date is three clicks, not a hundred.
	 * - Keyboard: arrows move a day or a week, PageUp/PageDown a month, Shift+PageUp/PageDown a
	 *   year, Home/End the start or end of the week, Enter picks, Escape closes.
	 * - The value is `YYYY-MM-DD` (or `YYYY-MM-DDTHH:MM` with `type="datetime"`), the same text a
	 *   browser date input produced, so nothing stored changes.
	 */
	let {
		value = $bindable(''),
		type = 'date',
		id,
		required = false,
		disabled = false,
		placeholder = 'Pick a date',
		min,
		max,
		class: className,
		onchange
	}: {
		value?: string;
		type?: 'date' | 'datetime';
		id?: string;
		required?: boolean;
		disabled?: boolean;
		placeholder?: string;
		/** Earliest selectable day (`YYYY-MM-DD`). */
		min?: string;
		/** Latest selectable day (`YYYY-MM-DD`). */
		max?: string;
		class?: string;
		onchange?: (value: string) => void;
	} = $props();

	const withTime = $derived(type === 'datetime');
	const firstDay = localeFirstDay();
	const MONTHS = Array.from({ length: 12 }, (_, i) =>
		new Intl.DateTimeFormat(undefined, { month: 'long' }).format(new Date(2021, i, 15))
	);
	const MONTHS_SHORT = Array.from({ length: 12 }, (_, i) =>
		new Intl.DateTimeFormat(undefined, { month: 'short' }).format(new Date(2021, i, 15))
	);
	const WEEKDAYS = Array.from({ length: 7 }, (_, i) => {
		// 2021-08-01 is a Sunday.
		const day = (firstDay + i) % 7;
		return new Intl.DateTimeFormat(undefined, { weekday: 'short' }).format(new Date(2021, 7, 1 + day));
	});

	const today = (() => {
		const now = new Date();
		return { y: now.getFullYear(), m: now.getMonth() + 1, d: now.getDate() } satisfies Ymd;
	})();

	const parsed = $derived(parseValue(value));
	const selected = $derived<Ymd | null>(parsed ? { y: parsed.y, m: parsed.m, d: parsed.d } : null);
	const minDay = $derived(min ? parseValue(min) : null);
	const maxDay = $derived(max ? parseValue(max) : null);

	let open = $state(false);
	let view = $state<'days' | 'months' | 'years'>('days');
	/** The day the keyboard is on (and the month shown). */
	let cursor = $state<Ymd>({ ...today });
	let hh = $state(0);
	let mm = $state(0);
	let draft = $state('');
	let editing = $state(false);
	let content = $state<HTMLElement | null>(null);

	const label = $derived.by(() => {
		if (!parsed) return '';
		const date = new Date(parsed.y, parsed.m - 1, parsed.d, parsed.hh, parsed.mm);
		return new Intl.DateTimeFormat(
			undefined,
			withTime
				? { dateStyle: 'medium', timeStyle: 'short' }
				: { year: 'numeric', month: 'short', day: 'numeric' }
		).format(date);
	});

	const cells = $derived(monthGrid(cursor.y, cursor.m, firstDay));
	const decadeStart = $derived(Math.floor(cursor.y / 12) * 12);

	function disabledDay(day: Ymd): boolean {
		return (minDay !== null && compare(day, minDay) < 0) || (maxDay !== null && compare(day, maxDay) > 0);
	}

	function emit(day: Ymd, hours = hh, minutes = mm) {
		const next = withTime ? formatDateTime({ ...day, hh: hours, mm: minutes }) : formatDate(day);
		value = next;
		onchange?.(next);
	}

	function clamp(day: Ymd): Ymd {
		if (minDay && compare(day, minDay) < 0) return { y: minDay.y, m: minDay.m, d: minDay.d };
		if (maxDay && compare(day, maxDay) > 0) return { y: maxDay.y, m: maxDay.m, d: maxDay.d };
		return day;
	}

	function choose(day: Ymd) {
		if (disabledDay(day)) return;
		emit(day);
		cursor = day;
		if (!withTime) open = false;
	}

	function clear() {
		value = '';
		onchange?.('');
		draft = '';
	}

	function goToday() {
		const day = clamp({ ...today });
		cursor = day;
		view = 'days';
		if (withTime) {
			const now = new Date();
			hh = now.getHours();
			mm = now.getMinutes();
		}
		emit(day, hh, mm);
		if (!withTime) open = false;
	}

	$effect(() => {
		if (!open) return;
		// Opening starts on the chosen day (or today) in the day view.
		const start = selected ?? clamp({ ...today });
		cursor = { ...start };
		view = 'days';
		hh = parsed?.hh ?? 0;
		mm = parsed?.mm ?? 0;
		void focusCursor();
	});

	async function focusCursor() {
		await tick();
		content?.querySelector<HTMLElement>('[data-cursor="true"]')?.focus();
	}

	function move(next: Ymd) {
		cursor = next;
		void focusCursor();
	}

	function onGridKey(event: KeyboardEvent) {
		const key = event.key;
		let next: Ymd | null = null;
		if (key === 'ArrowLeft') next = addDays(cursor, -1);
		else if (key === 'ArrowRight') next = addDays(cursor, 1);
		else if (key === 'ArrowUp') next = addDays(cursor, -7);
		else if (key === 'ArrowDown') next = addDays(cursor, 7);
		else if (key === 'PageUp') next = addMonths(cursor, event.shiftKey ? -12 : -1);
		else if (key === 'PageDown') next = addMonths(cursor, event.shiftKey ? 12 : 1);
		else if (key === 'Home' || key === 'End') {
			const dow = cells.find((c) => sameDay(c, cursor));
			const index = dow ? cells.indexOf(dow) % 7 : 0;
			next = addDays(cursor, key === 'Home' ? -index : 6 - index);
		}
		if (next) {
			event.preventDefault();
			move(next);
		}
	}

	function commitDraft() {
		editing = false;
		const text = draft.trim();
		if (!text) {
			if (value !== '') clear();
			return;
		}
		const day = parseTyped(text) ?? (parseValue(text) as Ymd | null);
		if (day && !disabledDay(day)) {
			emit(day, parsed?.hh ?? 0, parsed?.mm ?? 0);
		}
		draft = '';
	}

	function setTime() {
		const day = selected ?? clamp({ ...today });
		emit(day, Math.min(23, Math.max(0, hh)), Math.min(59, Math.max(0, mm)));
	}

	const title = $derived(
		view === 'days'
			? ''
			: view === 'months'
				? String(cursor.y)
				: `${decadeStart} – ${decadeStart + 11}`
	);
</script>

<Popover.Root bind:open>
	<!-- Looks like the other inputs of the app: same height, border, fill and focus ring. -->
	<div
		data-slot="date-picker"
		class={cn(
			'border-input bg-card shadow-xs focus-within:border-ring focus-within:ring-ring/20 flex h-9 w-full min-w-0 items-center rounded-md border text-base transition-[color,border-color] focus-within:ring-[3px] md:text-sm',
			disabled && 'pointer-events-none cursor-not-allowed opacity-50',
			className
		)}
	>
		<input
			{id}
			{required}
			{disabled}
			type="text"
			autocomplete="off"
			inputmode="numeric"
			{placeholder}
			class="placeholder:text-muted-foreground h-full min-w-0 flex-1 bg-transparent px-3 py-1 outline-none"
			value={editing ? draft : label}
			onfocus={() => {
				editing = true;
				draft = parsed ? formatDate(parsed) : '';
			}}
			oninput={(event) => (draft = event.currentTarget.value)}
			onblur={commitDraft}
			onkeydown={(event) => {
				if (event.key === 'Enter') {
					event.preventDefault();
					commitDraft();
				} else if (event.key === 'ArrowDown') {
					event.preventDefault();
					open = true;
				}
			}}
		/>
		{#if value && !required}
			<button
				type="button"
				class="text-muted-foreground hover:text-foreground flex h-full w-7 shrink-0 items-center justify-center"
				aria-label="Clear date"
				onclick={clear}
			>
				<XIcon class="size-3.5" />
			</button>
		{/if}
		<Popover.Trigger
			type="button"
			class="text-muted-foreground hover:text-foreground flex h-full w-9 shrink-0 items-center justify-center outline-none"
			aria-label="Open calendar"
		>
			<CalendarIcon class="size-4" />
		</Popover.Trigger>
	</div>

	<Popover.Content
		align="start"
		class="w-[16.5rem] gap-0 rounded-md p-0"
		bind:ref={content}
		onkeydown={(event: KeyboardEvent) => {
			if (event.key === 'Escape') open = false;
		}}
	>
		<!-- Caption: previous, month and year (each opens a faster way to jump), next. -->
		<div class="relative flex items-center justify-center px-3 pt-3 pb-2">
			<Button
				variant="outline"
				size="icon-xs"
				class="absolute left-3 size-7 bg-transparent p-0 opacity-70 hover:opacity-100"
				aria-label={view === 'days' ? 'Previous month' : view === 'months' ? 'Previous year' : 'Previous years'}
				onclick={() => {
					if (view === 'days') cursor = addMonths(cursor, -1);
					else if (view === 'months') cursor = addMonths(cursor, -12);
					else cursor = addMonths(cursor, -144);
				}}
			>
				<ChevronLeftIcon class="size-4" />
			</Button>
			<div class="flex items-center gap-0.5">
				{#if view === 'days'}
					<Button variant="ghost" size="xs" class="h-7 px-2 text-sm font-medium" onclick={() => (view = 'months')}>
						{MONTHS[cursor.m - 1]}
					</Button>
					<Button variant="ghost" size="xs" class="h-7 px-2 text-sm font-medium" onclick={() => (view = 'years')}>
						{cursor.y}
					</Button>
				{:else}
					<Button
						variant="ghost"
						size="xs"
						class="h-7 px-2 text-sm font-medium"
						onclick={() => (view = view === 'months' ? 'years' : 'days')}
					>
						{title}
					</Button>
				{/if}
			</div>
			<Button
				variant="outline"
				size="icon-xs"
				class="absolute right-3 size-7 bg-transparent p-0 opacity-70 hover:opacity-100"
				aria-label={view === 'days' ? 'Next month' : view === 'months' ? 'Next year' : 'Next years'}
				onclick={() => {
					if (view === 'days') cursor = addMonths(cursor, 1);
					else if (view === 'months') cursor = addMonths(cursor, 12);
					else cursor = addMonths(cursor, 144);
				}}
			>
				<ChevronRightIcon class="size-4" />
			</Button>
		</div>

		<div class="px-3 pb-3">
			{#if view === 'days'}
				<div class="mb-1 grid grid-cols-7 text-center">
					{#each WEEKDAYS as day (day)}
						<span class="text-muted-foreground flex h-8 items-center justify-center text-[0.8rem] font-normal">
							{day.slice(0, 2)}
						</span>
					{/each}
				</div>
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="grid grid-cols-7 gap-y-1" role="grid" onkeydown={onGridKey}>
					{#each cells as cell (`${cell.y}-${cell.m}-${cell.d}`)}
						{@const isSelected = sameDay(cell, selected)}
						{@const isToday = sameDay(cell, today)}
						{@const isCursor = sameDay(cell, cursor)}
						{@const blocked = disabledDay(cell)}
						<button
							type="button"
							role="gridcell"
							data-cursor={isCursor ? 'true' : undefined}
							tabindex={isCursor ? 0 : -1}
							disabled={blocked}
							aria-selected={isSelected}
							aria-current={isToday ? 'date' : undefined}
							class={cn(
								'mx-auto flex size-8 items-center justify-center rounded-md text-sm font-normal transition-colors outline-none',
								'hover:bg-accent hover:text-accent-foreground focus-visible:border-ring focus-visible:ring-ring/30 focus-visible:ring-2',
								!cell.inMonth && 'text-muted-foreground opacity-50',
								isToday && !isSelected && 'bg-accent text-accent-foreground',
								isSelected &&
									'bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground opacity-100',
								blocked && 'pointer-events-none opacity-30'
							)}
							onclick={() => choose(cell)}
						>
							{cell.d}
						</button>
					{/each}
				</div>
			{:else if view === 'months'}
				<div class="grid grid-cols-3 gap-1 pt-1">
					{#each MONTHS_SHORT as name, index (name)}
						<button
							type="button"
							class={cn(
								'hover:bg-accent hover:text-accent-foreground h-9 rounded-md text-sm font-normal transition-colors outline-none',
								'focus-visible:ring-ring/30 focus-visible:ring-2',
								today.y === cursor.y && today.m === index + 1 && 'bg-accent text-accent-foreground',
								cursor.m === index + 1 &&
									'bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground'
							)}
							onclick={() => {
								const month = addMonths({ y: cursor.y, m: 1, d: 1 }, index);
								cursor = { ...month, d: Math.min(selected?.d ?? cursor.d, 28) };
								view = 'days';
							}}
						>
							{name}
						</button>
					{/each}
				</div>
			{:else}
				<div class="grid grid-cols-3 gap-1 pt-1">
					{#each Array.from({ length: 12 }, (_, i) => decadeStart + i) as year (year)}
						<button
							type="button"
							class={cn(
								'hover:bg-accent hover:text-accent-foreground h-9 rounded-md text-sm font-normal transition-colors outline-none',
								'focus-visible:ring-ring/30 focus-visible:ring-2',
								today.y === year && 'bg-accent text-accent-foreground',
								cursor.y === year &&
									'bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground'
							)}
							onclick={() => {
								cursor = { y: year, m: cursor.m, d: Math.min(cursor.d, 28) };
								view = 'months';
							}}
						>
							{year}
						</button>
					{/each}
				</div>
			{/if}
		</div>

		{#if withTime && view === 'days'}
			<div class="flex items-center justify-between gap-2 border-t border-border px-3 py-2">
				<span class="text-muted-foreground text-sm">Time</span>
				<div class="flex items-center gap-1">
					<input
						type="number"
						min="0"
						max="23"
						aria-label="Hour"
						class="border-input bg-card shadow-xs focus-visible:border-ring focus-visible:ring-ring/20 h-8 w-14 rounded-md border px-2 text-center text-sm outline-none focus-visible:ring-[3px]"
						value={String(hh).padStart(2, '0')}
						onchange={(event) => {
							hh = Number(event.currentTarget.value) || 0;
							setTime();
						}}
					/>
					<span class="text-muted-foreground">:</span>
					<input
						type="number"
						min="0"
						max="59"
						aria-label="Minute"
						class="border-input bg-card shadow-xs focus-visible:border-ring focus-visible:ring-ring/20 h-8 w-14 rounded-md border px-2 text-center text-sm outline-none focus-visible:ring-[3px]"
						value={String(mm).padStart(2, '0')}
						onchange={(event) => {
							mm = Number(event.currentTarget.value) || 0;
							setTime();
						}}
					/>
				</div>
			</div>
		{/if}

		<div class="flex items-center justify-between border-t border-border p-2">
			<Button variant="ghost" size="sm" class="h-8 px-2 text-sm" onclick={goToday}>
				{withTime ? 'Now' : 'Today'}
			</Button>
			<div class="flex items-center gap-1">
				{#if value && !required}
					<Button
						variant="ghost"
						size="sm"
						class="text-muted-foreground h-8 px-2 text-sm"
						onclick={() => {
							clear();
							open = false;
						}}
					>
						Clear
					</Button>
				{/if}
				{#if withTime}
					<Button
						size="sm"
						class="h-8 px-3 text-sm"
						onclick={() => {
							if (!selected) emit(cursor);
							open = false;
						}}
					>
						Done
					</Button>
				{/if}
			</div>
		</div>
	</Popover.Content>
</Popover.Root>

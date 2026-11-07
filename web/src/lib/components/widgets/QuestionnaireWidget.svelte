<script lang="ts">
	import { goto } from '$app/navigation';
	import { onDestroy, untrack } from 'svelte';
	import { navHref } from '$lib/components/layout/nav';
	import type { PageNode } from '$lib/dsl/types';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';
	import {
		clock,
		firstMissing,
		isAnswered,
		payload,
		readQuestionnaire,
		toggle,
		type Answer,
		type Questionnaire
	} from '$lib/pages/questionnaire';

	/**
	 * `<questionnaire source="questionnaire" submit="submit_response" redirect="/forms"/>`: the questions of a form that is
	 * data, not page XML. `source` is called with the page's values and answers `{ questions: [{ id, label, kind,
	 * required, help, options }], locked?, intro?, anonymous?, thanks? }`; a `locked` text replaces the questions with
	 * that message. Kinds: text, long, choice, dropdown, multi, rating, yes_no, number, date. `submit` is called with the
	 * page's values and `{ answers: { id: answer } }`; after it works the `thanks` text is shown, and the page moves on
	 * to `redirect` (if any) a moment later.
	 *
	 * Timed mode (a quiz attempt): when `source` also answers `seconds_left`, a countdown runs from that number (the server's
	 * clock, not the browser's) and the answers are sent by themselves when it reaches zero; `saved` holds the answers already
	 * given. `save="save_answer"` is called, a moment after each change, with the page's values and `{ question, answer }`.
	 * `watch="report_violation"` is called with the page's values and `{ kind: 'left_tab' }` each time the person leaves the tab.
	 * A question may carry `points`, shown beside it.
	 */
	let { node }: { node: PageNode } = $props();
	const page = getPageState();

	let form = $state<Questionnaire | null>(null);
	let answers = $state<Record<string, Answer>>({});
	let error = $state<string | null>(null);
	let sent = $state<string | null>(null);
	let busy = $state(false);
	let latest = 0;
	let left = $state<number | null>(null);
	let ticker: ReturnType<typeof setInterval> | undefined;
	let saver: ReturnType<typeof setTimeout> | undefined;
	let lastSent: Record<string, string> = {};
	let autoSent = false;

	async function load() {
		const source = text(node.source);
		if (!source || !page) return;
		const current = ++latest;
		const result = await page.call(source, page.args);
		if (current !== latest) return;
		if (!result.ok) {
			error = result.error;
			return;
		}
		error = null;
		form = readQuestionnaire(result.data);
		answers = { ...form.saved };
		lastSent = Object.fromEntries(
			Object.entries(form.saved).map(([id, v]) => [id, JSON.stringify(v)])
		);
		left = form.secondsLeft;
		clearInterval(ticker);
		autoSent = false;
		if (left !== null && !form.locked) {
			const until = Date.now() + left * 1000;
			ticker = setInterval(() => {
				left = Math.max(0, Math.ceil((until - Date.now()) / 1000));
				if (left === 0) {
					clearInterval(ticker);
					void timeUp();
				}
			}, 500);
		}
	}

	async function timeUp() {
		if (autoSent || sent || !form) return;
		autoSent = true;
		await submitAnswers(true);
	}

	async function flush() {
		const save = text((node as Record<string, unknown>).save);
		if (!save || !page || !form) return;
		for (const q of form.questions) {
			const v = answers[q.id];
			const now = JSON.stringify(isAnswered(v) ? v : null);
			if ((lastSent[q.id] ?? 'null') === now) continue;
			lastSent[q.id] = now;
			const r = await page.call(save, {
				...page.args,
				question: q.id,
				answer: isAnswered(v) ? v : null
			});
			if (!r.ok) error = r.error;
		}
	}

	$effect(() => {
		void JSON.stringify(answers);
		untrack(() => {
			if (!text((node as Record<string, unknown>).save) || !form || form.locked || sent) return;
			clearTimeout(saver);
			saver = setTimeout(() => void flush(), 700);
		});
	});

	function leftTab() {
		const watch = text((node as Record<string, unknown>).watch);
		if (document.hidden && watch && page && form && !form.locked && !sent) {
			void page.call(watch, { ...page.args, kind: 'left_tab' });
		}
	}

	if (typeof document !== 'undefined') document.addEventListener('visibilitychange', leftTab);
	onDestroy(() => {
		clearInterval(ticker);
		clearTimeout(saver);
		if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', leftTab);
	});

	$effect(() => {
		void page?.plugin;
		void JSON.stringify(page?.args ?? {});
		untrack(() => void load());
	});

	async function send(event: SubmitEvent) {
		event.preventDefault();
		await submitAnswers(false);
	}

	async function submitAnswers(timeout: boolean) {
		const submit = text(node.submit);
		if (!form || !submit || !page || (busy && !timeout)) return;
		const missing = timeout ? null : firstMissing(form.questions, answers);
		if (missing) {
			error = `“${missing.label}” needs an answer.`;
			return;
		}
		clearTimeout(saver);
		busy = true;
		error = null;
		const r = await page.call(submit, { ...page.args, answers: payload(form.questions, answers) });
		busy = false;
		if (!r.ok) {
			error = r.error;
			return;
		}
		const reply = (r.data && typeof r.data === 'object' ? r.data : {}) as Record<string, unknown>;
		sent = typeof reply.thanks === 'string' && reply.thanks ? reply.thanks : form.thanks;
		const to = text(node.redirect);
		if (to) setTimeout(() => void goto(navHref(interpolate(to, page.vars ?? {}))), 1800);
	}

	const input =
		'h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring';
</script>

<section class="max-w-2xl space-y-5" data-part="questionnaire">
	{#if sent}
		<p class="rounded-md border border-border bg-muted/40 p-4 text-sm" role="status">{sent}</p>
	{:else if !form}
		{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
	{:else if form.locked}
		<p
			class="rounded-md border border-border bg-muted/40 p-4 text-sm"
			data-part="questionnaire-locked"
		>
			{form.locked}
		</p>
		{#if form.questions.length > 0 && form.locked.startsWith('This form is not open')}
			<ol class="list-decimal space-y-1 pl-5 text-sm text-muted-foreground">
				{#each form.questions as q (q.id)}<li>{q.label}</li>{/each}
			</ol>
		{/if}
	{:else}
		{#if form.intro}<p class="text-sm text-muted-foreground">{form.intro}</p>{/if}
		{#if form.anonymous}
			<p class="text-xs text-muted-foreground">Your answers are anonymous.</p>
		{/if}
		{#if left !== null}
			<p
				class="sticky top-2 z-10 w-fit rounded-md border border-border bg-background px-3 py-1 text-sm font-medium tabular-nums {left <
				60
					? 'text-destructive'
					: ''}"
				role="timer"
				aria-label="Time left"
				data-part="questionnaire-clock"
			>
				{clock(left)} left
			</p>
		{/if}
		<form class="space-y-6" onsubmit={send}>
			{#each form.questions as q, i (q.id)}
				<fieldset class="space-y-2" data-part="question" data-kind={q.kind}>
					<legend class="text-sm font-medium">
						{i + 1}. {q.label}{#if q.points !== null}<span
								class="ml-2 text-xs font-normal text-muted-foreground"
								>({q.points} {q.points === 1 ? 'mark' : 'marks'})</span
							>{/if}{#if q.required}<span class="text-destructive" aria-label="required">
								*</span
							>{/if}
					</legend>
					{#if q.help}<p class="text-xs text-muted-foreground">{q.help}</p>{/if}
					{#if q.kind === 'long'}
						<textarea
							class="{input} h-28 py-2"
							maxlength="5000"
							aria-label={q.label}
							bind:value={answers[q.id] as string}
						></textarea>
					{:else if q.kind === 'choice' || q.kind === 'yes_no'}
						{#each q.kind === 'yes_no' ? ['yes', 'no'] : q.options as option (option)}
							<label class="flex items-center gap-2 text-sm">
								<input
									type="radio"
									name={q.id}
									value={option}
									checked={answers[q.id] === option}
									onchange={() => (answers[q.id] = option)}
								/>
								<span class="capitalize">{option}</span>
							</label>
						{/each}
					{:else if q.kind === 'multi'}
						{#each q.options as option (option)}
							<label class="flex items-center gap-2 text-sm">
								<input
									type="checkbox"
									checked={Array.isArray(answers[q.id]) &&
										(answers[q.id] as string[]).includes(option)}
									onchange={(e) =>
										(answers[q.id] = toggle(answers[q.id], option, e.currentTarget.checked))}
								/>
								{option}
							</label>
						{/each}
					{:else if q.kind === 'dropdown'}
						<select class={input} aria-label={q.label} bind:value={answers[q.id] as string}>
							<option value="">Choose…</option>
							{#each q.options as option (option)}<option value={option}>{option}</option>{/each}
						</select>
					{:else if q.kind === 'rating'}
						<div class="flex gap-1" role="radiogroup" aria-label={q.label}>
							{#each [1, 2, 3, 4, 5] as n (n)}
								<button
									type="button"
									role="radio"
									aria-checked={answers[q.id] === n}
									aria-label="{n} of 5"
									class="size-9 rounded-md border text-sm {answers[q.id] === n
										? 'border-primary bg-primary text-primary-foreground'
										: 'border-border hover:bg-muted'}"
									onclick={() => (answers[q.id] = answers[q.id] === n ? undefined : n)}>{n}</button
								>
							{/each}
						</div>
					{:else if q.kind === 'number'}
						<input
							type="number"
							step="any"
							class={input}
							aria-label={q.label}
							bind:value={answers[q.id]}
						/>
					{:else if q.kind === 'date'}
						<input
							type="date"
							class={input}
							aria-label={q.label}
							bind:value={answers[q.id] as string}
						/>
					{:else}
						<input
							type="text"
							class={input}
							maxlength="500"
							aria-label={q.label}
							bind:value={answers[q.id] as string}
						/>
					{/if}
				</fieldset>
			{/each}
			{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
			<button
				type="submit"
				class="inline-flex h-9 items-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
				disabled={busy || !text(node.submit)}>{text(node.label) ?? 'Send'}</button
			>
		</form>
	{/if}
</section>

<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Badge } from '$lib/components/ui/badge';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import {
		fetchScheduleJob,
		formatDuration,
		type JobDetail
	} from '$lib/schedules/api';

	let {
		open = $bindable(false),
		db,
		jobId = $bindable(null)
	}: {
		open?: boolean;
		db: string;
		jobId?: string | null;
	} = $props();

	let detail = $state<JobDetail | null>(null);
	let failed = $state<string | null>(null);
	let loading = $state(false);

	$effect(() => {
		if (!open || !jobId) {
			detail = null;
			failed = null;
			loading = false;
			return;
		}
		const id = jobId;
		let cancelled = false;
		loading = true;
		failed = null;
		detail = null;
		void fetchScheduleJob(db, id)
			.then((result) => {
				if (cancelled) return;
				detail = result;
			})
			.catch((error) => {
				if (cancelled) return;
				failed = error instanceof Error ? error.message : 'The run could not be loaded.';
			})
			.finally(() => {
				if (!cancelled) loading = false;
			});
		return () => {
			cancelled = true;
		};
	});

	function formatWhen(iso: string | null | undefined): string {
		if (!iso) return '—';
		const date = new Date(iso);
		return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
	}

	function stateVariant(state: string): 'default' | 'secondary' | 'outline' | 'destructive' {
		if (state === 'succeeded') return 'default';
		if (state === 'failed') return 'destructive';
		if (state === 'running') return 'secondary';
		return 'outline';
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="flex max-h-[90vh] max-w-2xl flex-col gap-0 overflow-hidden p-0">
		<Dialog.Header class="border-b border-border px-6 py-4">
			<Dialog.Title>Run</Dialog.Title>
			<Dialog.Description class="font-mono text-xs">
				{jobId ?? ''}
			</Dialog.Description>
		</Dialog.Header>

		<div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
			{#if loading}
				<div class="space-y-2">
					<Skeleton class="h-4 w-1/2" />
					<Skeleton class="h-4 w-2/3" />
					<Skeleton class="h-20 w-full" />
				</div>
			{:else if failed}
				<p class="text-sm text-destructive" role="alert">{failed}</p>
			{:else if detail}
				{@const job = detail.job}
				<div class="flex flex-wrap items-center gap-2">
					<Badge variant={stateVariant(job.state)}>{job.state}</Badge>
					<span class="text-sm text-muted-foreground">
						Attempt {job.attempts}/{job.max_attempts}
					</span>
				</div>
				<dl class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
					<div>
						<dt class="text-xs text-muted-foreground">Started</dt>
						<dd>{formatWhen(job.date_started)}</dd>
					</div>
					<div>
						<dt class="text-xs text-muted-foreground">Ended</dt>
						<dd>{formatWhen(job.date_finished)}</dd>
					</div>
					<div>
						<dt class="text-xs text-muted-foreground">Duration</dt>
						<dd>{formatDuration(job.duration_ms)}</dd>
					</div>
					<div>
						<dt class="text-xs text-muted-foreground">Queue</dt>
						<dd class="font-mono text-xs">{job.queue}</dd>
					</div>
				</dl>
				{#if job.last_error}
					<div class="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
						{job.last_error}
					</div>
				{/if}

				<section class="space-y-2">
					<h3 class="text-sm font-medium">Plugin calls</h3>
					{#if detail.logs.plugin_calls.length === 0}
						<p class="text-sm text-muted-foreground">No plugin calls recorded for this run.</p>
					{:else}
						<ul class="divide-y divide-border rounded-md border border-border">
							{#each detail.logs.plugin_calls as call (call.date_created + call.plugin + call.function)}
								<li class="flex flex-wrap items-baseline justify-between gap-2 px-3 py-2 text-sm">
									<span class="font-mono text-xs">
										{call.plugin}.{call.function}
									</span>
									<span class="text-xs text-muted-foreground">
										status {call.status} · {formatWhen(call.date_created)}
									</span>
								</li>
							{/each}
						</ul>
					{/if}
				</section>

				<section class="space-y-2">
					<h3 class="text-sm font-medium">Data access</h3>
					{#if detail.logs.data_access.length === 0}
						<p class="text-sm text-muted-foreground">No data access recorded for this run.</p>
					{:else}
						<ul class="divide-y divide-border rounded-md border border-border">
							{#each detail.logs.data_access as access, i (i)}
								<li class="space-y-0.5 px-3 py-2 text-sm">
									<div class="flex flex-wrap items-baseline justify-between gap-2">
										<span class="font-mono text-xs">
											{access.operation}
											{#if access.table || access.model}
												· {access.table ?? access.model}
											{/if}
										</span>
										<span class="text-xs text-muted-foreground">
											{access.record_count} row(s) · {formatWhen(access.date_created)}
										</span>
									</div>
									{#if access.statement}
										<pre class="overflow-x-auto text-xs text-muted-foreground">{access.statement}</pre>
									{/if}
								</li>
							{/each}
						</ul>
					{/if}
				</section>
			{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>

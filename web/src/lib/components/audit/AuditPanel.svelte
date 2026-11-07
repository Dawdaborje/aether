<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect, NativeSelectOption } from '$lib/components/ui/native-select';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import {
		AUDIT_TABLES,
		actorLabel,
		chainSummary,
		fetchAudit,
		fetchAuditChain,
		isRefused,
		summarize,
		verifyAuditChain,
		type AuditChainStatus,
		type AuditPage,
		type AuditRow,
		type AuditTable
	} from '$lib/audit/api';
	import { loadFailure } from '$lib/errors/report';
	import { choiceParam, numberParam, textParam } from '$lib/url/filterParams';
	import { syncFilters } from '$lib/url/syncFilters.svelte';

	let { db }: { db: string } = $props();

	const PAGE = 50;
	const WITHIN = [0, 1, 24, 168, 720] as const;

	// The filters live in the address bar, so a view can be shared or reloaded.
	const urlParams = page.url.searchParams;
	let table = $state<AuditTable>(
		choiceParam(urlParams, 'table', ['data_access', 'plugin_calls', 'page_visits'], 'data_access')
	);
	let query = $state(textParam(urlParams, 'q'));
	let actor = $state(textParam(urlParams, 'actor'));
	let plugin = $state(textParam(urlParams, 'plugin'));
	let request = $state(textParam(urlParams, 'request'));
	let operation = $state(textParam(urlParams, 'op'));
	let failedOnly = $state(urlParams.get('failed') === 'true');
	let within = $state<number>(numberParam(urlParams, 'within', WITHIN, 24));
	syncFilters(
		() => ({
			table,
			q: query,
			actor,
			plugin,
			request,
			op: operation,
			failed: failedOnly ? 'true' : '',
			within
		}),
		{ table: 'data_access', within: 24 }
	);

	let result = $state<AuditPage | null>(null);
	let failed = $state<string | null>(null);
	let loading = $state(false);
	let offset = $state(0);
	let expanded = $state<string | null>(null);

	let chain = $state<AuditChainStatus | null>(null);
	let verifying = $state(false);
	let verifyNote = $state<string | null>(null);

	let token = 0;
	async function load() {
		const mine = ++token;
		loading = true;
		try {
			const next = await fetchAudit(db, {
				table,
				limit: PAGE,
				offset,
				q: query,
				actor,
				plugin,
				request,
				operation: table === 'data_access' ? operation : '',
				failed: table !== 'data_access' && failedOnly,
				withinHours: within
			});
			if (mine !== token) return;
			result = next;
			failed = null;
		} catch (error) {
			if (mine !== token) return;
			failed = loadFailure(error, 'The audit log could not be loaded.');
		} finally {
			if (mine === token) loading = false;
		}
	}

	async function loadChain() {
		try {
			chain = await fetchAuditChain(db);
		} catch {
			chain = null;
		}
	}

	async function verifyNow() {
		verifying = true;
		verifyNote = null;
		try {
			await verifyAuditChain(db);
			await loadChain();
		} catch (error) {
			verifyNote = loadFailure(error, 'The check could not run.');
		} finally {
			verifying = false;
		}
	}

	onMount(() => {
		void load();
		void loadChain();
	});

	// Reload when a filter changes; typing waits for a pause.
	let timer: ReturnType<typeof setTimeout> | undefined;
	let first = true;
	$effect(() => {
		void [table, query, actor, plugin, request, operation, failedOnly, within];
		if (first) {
			first = false;
			return;
		}
		clearTimeout(timer);
		timer = setTimeout(() => {
			offset = 0;
			expanded = null;
			void load();
		}, 250);
		return () => clearTimeout(timer);
	});

	function go(next: number) {
		offset = next;
		expanded = null;
		void load();
	}

	function showRequest(id: string) {
		request = id;
	}

	const filtersActive = $derived(
		Boolean(query.trim() || actor.trim() || plugin.trim() || request || operation || failedOnly) ||
			within !== 24
	);
	function clearFilters() {
		query = '';
		actor = '';
		plugin = '';
		request = '';
		operation = '';
		failedOnly = false;
		within = 24;
	}

	const summary = $derived(chainSummary(chain));
	const rows = $derived(result?.rows ?? []);
	const total = $derived(result?.total ?? 0);
	const tableInfo = $derived(AUDIT_TABLES.find((t) => t.id === table));

	function when(iso: string): string {
		const d = new Date(iso);
		return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
	}

	function emptyMessage(): string {
		if (!filtersActive) {
			return 'Nothing has been recorded here yet. Rows appear as people use the organization.';
		}
		const bits = [
			query.trim() && `text “${query.trim()}”`,
			actor.trim() && `actor ${actor.trim()}`,
			plugin.trim() && `plugin ${plugin.trim()}`,
			request && `request ${request}`,
			operation && `operation ${operation}`,
			failedOnly && 'only refused or failed',
			within === 0
				? ''
				: within === 1
					? 'in the last hour'
					: within < 48
						? `in the last ${within} hours`
						: `in the last ${within / 24} days`
		].filter(Boolean);
		return `No ${tableInfo?.label.toLowerCase() ?? 'rows'} match: ${bits.join(', ')}. Use Clear to remove the filters.`;
	}

	function details(row: AuditRow): [string, string][] {
		const out: [string, string][] = [
			['Time', when(row.date_created)],
			['Actor', actorLabel(row)],
			['Request', row.request_id]
		];
		if (row.plugin) out.push(['Plugin', row.plugin]);
		if (row.function_name) out.push(['Function', row.function_name]);
		if (row.model) out.push(['Model', row.model]);
		if (row.table_name) out.push(['Table', row.table_name]);
		if (row.operation) out.push(['Operation', row.operation]);
		if (row.record_ids?.length) out.push(['Records', row.record_ids.join(', ')]);
		if (row.statement) out.push(['Statement', row.statement]);
		if (row.route) out.push(['Route', row.route]);
		if (row.path) out.push(['Path', row.path]);
		if (row.status != null) out.push(['Status', String(row.status)]);
		if (row.ip) out.push(['Address', row.ip]);
		if (row.user_agent) out.push(['Browser', row.user_agent]);
		if (row.chain_seq != null) out.push(['Chain number', String(row.chain_seq)]);
		return out;
	}
</script>

<div class="space-y-4" data-testid="audit-log">
	<div
		class="flex flex-wrap items-center gap-3 rounded-md border border-border bg-card p-3 text-sm"
		role="status"
	>
		{#if summary.ok === true}
			<AppIcon
				name="check-circle-filled"
				class="size-5 shrink-0 text-emerald-600 dark:text-emerald-400"
				aria-hidden="true"
			/>
		{:else}
			<AppIcon
				name="warning-filled"
				class="size-5 shrink-0 {summary.ok === false
					? 'text-destructive'
					: 'text-amber-600 dark:text-amber-400'}"
				aria-hidden="true"
			/>
		{/if}
		<span class="min-w-0 flex-1">{summary.text}</span>
		<Button size="sm" variant="outline" onclick={verifyNow} disabled={verifying}>
			{verifying ? 'Checking…' : 'Check now'}
		</Button>
		{#if verifyNote}
			<p class="w-full text-destructive" role="alert">{verifyNote}</p>
		{/if}
	</div>

	<div class="flex flex-wrap gap-1" role="tablist" aria-label="Audit table">
		{#each AUDIT_TABLES as t (t.id)}
			<Button
				size="sm"
				variant={table === t.id ? 'default' : 'outline'}
				role="tab"
				aria-selected={table === t.id}
				title={t.hint}
				onclick={() => (table = t.id)}>{t.label}</Button
			>
		{/each}
	</div>

	<div class="flex flex-wrap items-center gap-2">
		<div class="relative min-w-48 flex-1">
			<AppIcon
				name="search"
				class="pointer-events-none absolute top-2.5 left-2.5 size-4 text-muted-foreground"
			/>
			<Input class="pl-8" placeholder="Search function, model, path…" bind:value={query} />
		</div>
		<Input class="w-44" placeholder="Actor (user id)" aria-label="Actor" bind:value={actor} />
		<Input class="w-36" placeholder="Plugin" aria-label="Plugin" bind:value={plugin} />
		{#if table === 'data_access'}
			<NativeSelect
				class="w-36"
				aria-label="Operation"
				value={operation}
				onchange={(event) => (operation = event.currentTarget.value)}
			>
				<NativeSelectOption value="">Any operation</NativeSelectOption>
				<NativeSelectOption value="read">Read</NativeSelectOption>
				<NativeSelectOption value="create">Create</NativeSelectOption>
				<NativeSelectOption value="update">Update</NativeSelectOption>
				<NativeSelectOption value="delete">Delete</NativeSelectOption>
				<NativeSelectOption value="raw">Raw query</NativeSelectOption>
			</NativeSelect>
		{:else}
			<label class="flex items-center gap-2 text-sm">
				<input type="checkbox" bind:checked={failedOnly} class="size-4" />
				Only refused or failed
			</label>
		{/if}
		<NativeSelect
			class="w-44"
			aria-label="Time range"
			value={String(within)}
			onchange={(event) => (within = Number(event.currentTarget.value))}
		>
			<NativeSelectOption value="1">Last hour</NativeSelectOption>
			<NativeSelectOption value="24">Last 24 hours</NativeSelectOption>
			<NativeSelectOption value="168">Last 7 days</NativeSelectOption>
			<NativeSelectOption value="720">Last 30 days</NativeSelectOption>
			<NativeSelectOption value="0">Any time</NativeSelectOption>
		</NativeSelect>
		{#if filtersActive}
			<Button size="sm" variant="ghost" onclick={clearFilters}>Clear</Button>
		{/if}
	</div>

	{#if request}
		<p class="text-sm text-muted-foreground">
			Showing rows of request <span class="font-mono">{request}</span>. Switch tabs to see what else
			that request did.
			<button class="underline" onclick={() => (request = '')}>Show all</button>
		</p>
	{/if}

	{#if failed && !result}
		<LoadFailed message={failed} onRetry={load} retrying={loading} />
	{:else if !result}
		<Skeleton class="h-64 w-full" />
	{:else if rows.length === 0}
		<p class="text-sm text-muted-foreground">{emptyMessage()}</p>
	{:else}
		{#if failed}
			<p class="text-sm text-destructive" role="alert">{failed} Showing the last result.</p>
		{/if}
		<div class="overflow-x-auto rounded-md border border-border">
			<table class="w-full min-w-[720px] text-sm">
				<thead class="bg-muted/50 text-left text-xs text-muted-foreground">
					<tr>
						<th class="w-8 px-2 py-2"></th>
						<th class="px-3 py-2 font-medium">Time</th>
						<th class="px-3 py-2 font-medium">Actor</th>
						<th class="px-3 py-2 font-medium">What</th>
						<th class="px-3 py-2 font-medium">Plugin</th>
						<th class="px-3 py-2 font-medium">Result</th>
					</tr>
				</thead>
				<tbody>
					{#each rows as row (row.id)}
						{@const open = expanded === row.id}
						<tr
							class="cursor-pointer border-t border-border hover:bg-muted/40"
							onclick={() => (expanded = open ? null : row.id)}
						>
							<td class="px-2 py-2">
								<button
									class="flex"
									aria-expanded={open}
									aria-label={open ? 'Hide details' : 'Show details'}
									onclick={(event) => {
										event.stopPropagation();
										expanded = open ? null : row.id;
									}}
								>
									<AppIcon
										name="chevron-right"
										class="size-4 transition-transform {open ? 'rotate-90' : ''}"
									/>
								</button>
							</td>
							<td class="px-3 py-2 whitespace-nowrap">{when(row.date_created)}</td>
							<td class="px-3 py-2">{actorLabel(row)}</td>
							<td class="px-3 py-2">{summarize(table, row)}</td>
							<td class="px-3 py-2">{row.plugin ?? '—'}</td>
							<td class="px-3 py-2">
								{#if row.status != null}
									<Badge variant={isRefused(row) ? 'destructive' : 'outline'}>{row.status}</Badge>
								{:else}
									<span class="text-muted-foreground">—</span>
								{/if}
							</td>
						</tr>
						{#if open}
							<tr class="border-t border-border bg-muted/20">
								<td></td>
								<td colspan="5" class="px-3 py-3">
									<dl class="grid gap-x-4 gap-y-1 text-sm sm:grid-cols-[8rem_minmax(0,1fr)]">
										{#each details(row) as [label, value] (label)}
											<dt class="text-muted-foreground">{label}</dt>
											<dd
												class="break-all {label === 'Statement' || label === 'Request'
													? 'font-mono text-xs'
													: ''}"
											>
												{value}
											</dd>
										{/each}
									</dl>
									<Button
										class="mt-2"
										size="sm"
										variant="outline"
										onclick={() => showRequest(row.request_id)}>Show this request</Button
									>
								</td>
							</tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
		<div class="flex items-center justify-between gap-2 text-sm text-muted-foreground">
			<span>{offset + 1}–{offset + rows.length} of {total}</span>
			<div class="flex gap-2">
				<Button
					size="sm"
					variant="outline"
					disabled={loading || offset === 0}
					onclick={() => go(Math.max(0, offset - PAGE))}>Previous</Button
				>
				<Button
					size="sm"
					variant="outline"
					disabled={loading || offset + rows.length >= total}
					onclick={() => go(offset + PAGE)}>Next</Button
				>
			</div>
		</div>
	{/if}
</div>

<script lang="ts">
	import { reportError } from '$lib/errors/report';
	import { onMount } from 'svelte';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import { fetchPluginActivity, type PluginActivityEvent } from '$lib/org/api';

	let { params }: { params: { db: string } } = $props();

	const PAGE = 100;
	let events = $state<PluginActivityEvent[] | null>(null);
	let loading = $state(false);
	let failed = $state<string | null>(null);
	let more = $state(true);
	let kindFilter = $state<'all' | 'catalog_load' | 'install' | 'upgrade'>('all');

	const KINDS: Record<string, string> = {
		catalog_load: 'Loaded from disk',
		install: 'Installed',
		upgrade: 'Upgraded'
	};

	const SOURCES: Record<string, string> = {
		'desk:reload': 'Desk — reload from disk',
		'desk:reload_upgrade': 'Desk — reload & upgrade',
		'desk:upload': 'Desk — zip upload',
		'desk:install': 'Desk — install',
		'desk:upgrade': 'Desk — upgrade modules',
		'desk:model_edit': 'Desk — model edit',
		'cli:load': 'Command line — load',
		'cli:install': 'Command line — install',
		'cli:upgrade': 'Command line — upgrade',
		watch: 'File watcher'
	};

	async function load(reset: boolean) {
		if (loading) return;
		loading = true;
		failed = null;
		try {
			const offset = reset ? 0 : (events?.length ?? 0);
			const next = await fetchPluginActivity(params.db, { limit: PAGE, offset });
			events = reset ? next : [...(events ?? []), ...next];
			more = next.length === PAGE;
		} catch (error) {
			failed = reportError(error, 'The activity could not be loaded.');
			events ??= [];
		} finally {
			loading = false;
		}
	}

	onMount(() => void load(true));

	const shown = $derived(
		(events ?? []).filter((event) => kindFilter === 'all' || event.kind === kindFilter)
	);

	function when(value: string | null): string {
		if (!value) return '';
		const date = new Date(value);
		return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
	}

	function version(event: PluginActivityEvent): string {
		if (event.from_version && event.to_version) return `${event.from_version} → ${event.to_version}`;
		return event.to_version ?? event.from_version ?? '';
	}

	function who(event: PluginActivityEvent): string {
		if (event.actor_name) return event.actor_name;
		return event.actor_kind === 'system' ? 'system' : 'unknown';
	}
</script>

<div class="space-y-4 py-2">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<p class="max-w-2xl text-sm text-muted-foreground">
			Who loaded plugins from disk, and who installed or upgraded them in this organization. Loads
			from disk apply to every organization, so they appear here too. Nothing is ever removed from
			this log.
		</p>
		<div class="flex items-center gap-2">
			<select
				class="h-9 rounded-sm border border-input bg-card px-2 text-sm"
				aria-label="Filter by event"
				bind:value={kindFilter}
			>
				<option value="all">All events</option>
				<option value="catalog_load">Loaded from disk</option>
				<option value="install">Installed</option>
				<option value="upgrade">Upgraded</option>
			</select>
			<Button
				size="sm"
				variant="ghost"
				title="Refresh"
				aria-label="Refresh"
				disabled={loading}
				onclick={() => void load(true)}
			>
				<RefreshCwIcon class={loading ? 'size-4 animate-spin' : 'size-4'} />
			</Button>
		</div>
	</div>

	{#if failed}
		<p class="text-sm text-destructive" role="alert">{failed}</p>
	{/if}

	{#if events === null}
		<p class="text-sm text-muted-foreground">Loading…</p>
	{:else if shown.length === 0}
		<p
			class="rounded-md border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground"
		>
			No plugin activity recorded yet.
		</p>
	{:else}
		<div class="overflow-x-auto rounded-lg border border-border">
			<table class="w-full text-sm">
				<thead class="bg-muted/40 text-left text-xs text-muted-foreground">
					<tr>
						<th class="px-3 py-2 font-medium">When</th>
						<th class="px-3 py-2 font-medium">Event</th>
						<th class="px-3 py-2 font-medium">Plugin</th>
						<th class="px-3 py-2 font-medium">Version</th>
						<th class="px-3 py-2 font-medium">By</th>
						<th class="px-3 py-2 font-medium">From</th>
						<th class="px-3 py-2 font-medium">Result</th>
					</tr>
				</thead>
				<tbody>
					{#each shown as event, index (index + (event.at ?? ''))}
						<tr class="border-t border-border align-top">
							<td class="px-3 py-2 whitespace-nowrap text-muted-foreground">{when(event.at)}</td>
							<td class="px-3 py-2">{KINDS[event.kind] ?? event.kind}</td>
							<td class="px-3 py-2 font-mono text-xs">{event.plugin}</td>
							<td class="px-3 py-2 font-mono text-xs">{version(event)}</td>
							<td class="px-3 py-2">{who(event)}</td>
							<td class="px-3 py-2 text-muted-foreground">
								{SOURCES[event.source ?? ''] ?? event.source ?? ''}
							</td>
							<td class="px-3 py-2">
								{#if event.ok === false}
									<Badge variant="destructive">Failed</Badge>
									{#if event.error}
										<p class="mt-1 max-w-xs text-xs text-destructive">{event.error}</p>
									{/if}
								{:else}
									<Badge variant="secondary">Done</Badge>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if more}
			<div class="flex justify-center">
				<Button size="sm" variant="outline" disabled={loading} onclick={() => void load(false)}>
					{loading ? 'Loading…' : 'Load older events'}
				</Button>
			</div>
		{/if}
	{/if}
</div>

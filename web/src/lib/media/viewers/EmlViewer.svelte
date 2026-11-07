<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { formatSize } from '../api';
	import { parseEmail, type Email } from './eml';
	import SafeHtmlFrame from './SafeHtmlFrame.svelte';
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let mail = $state<Email | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(true);

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		current
			.text(5_000_000)
			.then((body) => (mail = parseEmail(body.text)))
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'))
			.finally(() => (loading = false));
	});

	const rows = $derived(
		mail
			? (
					[
						['From', mail.headers.from],
						['To', mail.headers.to],
						['Cc', mail.headers.cc],
						['Date', mail.headers.date]
					] as const
				).filter(([, v]) => v)
			: []
	);
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else if mail}
	<div class="flex h-full flex-col bg-background">
		<header class="shrink-0 space-y-1 border-b border-border p-3 text-sm">
			<h2 class="text-base font-semibold break-words">{mail.headers.subject || '(no subject)'}</h2>
			<dl class="grid grid-cols-[auto_1fr] gap-x-3 text-xs">
				{#each rows as [name, value] (name)}
					<dt class="text-muted-foreground">{name}</dt>
					<dd class="break-words">{value}</dd>
				{/each}
			</dl>
		</header>
		<div class="min-h-0 flex-1 overflow-auto">
			{#if mail.html}
				<SafeHtmlFrame html={mail.html} />
			{:else}
				<pre class="p-3 text-sm break-words whitespace-pre-wrap">{mail.text || '(no text)'}</pre>
			{/if}
		</div>
		{#if mail.attachments.length > 0}
			<footer class="shrink-0 border-t border-border p-2 text-xs">
				<p class="mb-1 flex items-center gap-1 text-muted-foreground">
					<AppIcon name="attachment" class="size-3" />
					{mail.attachments.length} attachment{mail.attachments.length === 1 ? '' : 's'} (not opened)
				</p>
				<ul class="space-y-0.5">
					{#each mail.attachments as a, i (i)}<li class="break-all">
							{a.name} <span class="text-muted-foreground">· {a.type} · ~{formatSize(a.size)}</span>
						</li>{/each}
				</ul>
			</footer>
		{/if}
	</div>
{/if}

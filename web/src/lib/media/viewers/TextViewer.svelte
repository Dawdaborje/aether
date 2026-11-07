<script lang="ts">
	import type { ViewerContext } from './types';

	let { ctx }: { ctx: ViewerContext } = $props();
	let text = $state('');
	let cut = $state(false);
	let error = $state<string | null>(null);
	let loading = $state(true);

	$effect(() => {
		const current = ctx;
		loading = true;
		error = null;
		current.setPrint(() => window.print());
		current
			.text()
			.then((body) => {
				cut = body.cut;
				const ext = current.file.name.toLowerCase().split('.').pop();
				if (ext === 'json' || current.file.mime === 'application/json') {
					try {
						text = JSON.stringify(JSON.parse(body.text), null, 2);
						return;
					} catch {
						/* show it as it is */
					}
				}
				text = body.text;
			})
			.catch((e) => (error = e instanceof Error ? e.message : 'The file could not be opened.'))
			.finally(() => (loading = false));
		return () => current.setPrint(null);
	});
</script>

{#if loading}
	<p class="p-4 text-sm text-muted-foreground">Opening…</p>
{:else if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{:else}
	<pre class="p-3 text-xs break-words whitespace-pre-wrap">{text}</pre>
	{#if cut}<p class="p-2 text-xs text-muted-foreground">
			Showing the start of a large file. Download for all of it.
		</p>{/if}
{/if}

<script lang="ts">
	import { apiFetch } from '$lib/api/client';
	import { downloadMedia } from '../api';
	import { sandboxDocument } from './sandbox';
	import type { PluginViewerSpec, ViewerContext } from './types';

	let { ctx, spec }: { ctx: ViewerContext; spec?: PluginViewerSpec } = $props();
	let frame = $state<HTMLIFrameElement | null>(null);
	let error = $state<string | null>(null);
	let ready = $state(false);

	const doc = $derived(sandboxDocument(spec?.hosts ?? []));

	// The frame has no session, so the app fetches the module and the file and hands them over.
	async function start(
		target: HTMLIFrameElement,
		current: ViewerContext,
		plugin: PluginViewerSpec
	) {
		try {
			const [response, bytes] = await Promise.all([apiFetch(plugin.moduleUrl), current.bytes()]);
			if (!response.ok) throw new Error('The viewer could not be loaded.');
			const code = await response.text();
			target.contentWindow?.postMessage(
				{
					t: 'init',
					file: { name: current.file.name, mime: current.file.mime, size: current.file.size },
					code,
					// A copy: the file stays available to the other actions.
					bytes: bytes.slice(0)
				},
				'*'
			);
			ready = true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The file could not be opened.';
		}
	}

	$effect(() => {
		const target = frame;
		const current = ctx;
		const plugin = spec;
		if (!target || !plugin) return;
		error = null;
		ready = false;
		const onMessage = (event: MessageEvent) => {
			// Only the frame this viewer owns is listened to.
			if (event.source !== target.contentWindow) return;
			const data = event.data as { t?: string; message?: string; action?: string } | null;
			if (data?.t === 'ready') void start(target, current, plugin);
			else if (data?.t === 'error') error = String(data.message ?? 'The viewer failed.');
			else if (data?.t === 'action') {
				if (data.action === 'download') void downloadMedia(current.file);
				else if (data.action === 'print') target.contentWindow?.print();
			}
		};
		window.addEventListener('message', onMessage);
		current.setPrint(() => target.contentWindow?.print());
		return () => {
			window.removeEventListener('message', onMessage);
			current.setPrint(null);
		};
	});
</script>

{#if error}
	<p class="p-4 text-sm text-destructive" role="alert">{error}</p>
{/if}
<iframe
	bind:this={frame}
	title={ctx.file.name}
	sandbox="allow-scripts"
	srcdoc={doc}
	class="h-full w-full border-0 bg-background {error ? 'hidden' : ''}"
	data-ready={ready}
></iframe>

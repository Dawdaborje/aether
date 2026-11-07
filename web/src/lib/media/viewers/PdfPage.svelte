<script lang="ts">
	import type { PDFDocumentProxy, RenderTask } from 'pdfjs-dist';

	/**
	 * One page drawn on a canvas. It reserves its size at once and draws only while it is near the
	 * visible area, so a long document does not hold hundreds of canvases.
	 */
	let {
		doc,
		number,
		scale,
		root = null
	}: {
		doc: PDFDocumentProxy;
		number: number;
		scale: number;
		root?: HTMLElement | null;
	} = $props();

	let box = $state<HTMLDivElement | null>(null);
	let canvas = $state<HTMLCanvasElement | null>(null);
	let size = $state({ width: 612, height: 792 });
	let near = $state(false);
	let failed = $state(false);

	$effect(() => {
		const target = doc;
		const n = number;
		const s = scale;
		let alive = true;
		target
			.getPage(n)
			.then((page) => {
				if (!alive) return;
				const view = page.getViewport({ scale: s });
				size = { width: view.width, height: view.height };
			})
			.catch(() => {});
		return () => (alive = false);
	});

	$effect(() => {
		if (!box) return;
		const observer = new IntersectionObserver(([entry]) => (near = entry.isIntersecting), {
			root,
			rootMargin: '600px 0px'
		});
		observer.observe(box);
		return () => observer.disconnect();
	});

	$effect(() => {
		const target = canvas;
		if (!near || !target) return;
		const dpr = window.devicePixelRatio || 1;
		let task: RenderTask | null = null;
		let alive = true;
		failed = false;
		doc
			.getPage(number)
			.then((page) => {
				if (!alive) return;
				const view = page.getViewport({ scale: scale * dpr });
				target.width = Math.floor(view.width);
				target.height = Math.floor(view.height);
				task = page.render({ canvas: target, viewport: view });
				return task.promise;
			})
			.catch((e) => {
				if (alive && e?.name !== 'RenderingCancelledException') failed = true;
			});
		return () => {
			alive = false;
			task?.cancel();
			// Drawn pages that scroll far away give their memory back.
			target.width = 0;
			target.height = 0;
		};
	});
</script>

<div
	bind:this={box}
	data-page={number}
	class="relative mx-auto bg-white shadow-sm"
	style="width:{size.width}px;height:{size.height}px"
>
	<canvas bind:this={canvas} class="absolute inset-0 h-full w-full"></canvas>
	{#if failed}<p class="absolute inset-0 grid place-items-center text-xs text-destructive">
			Page {number} could not be drawn.
		</p>{/if}
</div>

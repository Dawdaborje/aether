<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { Button } from '$lib/components/ui/button';
	import { uiPrefs } from '$lib/theme/uiPrefs.svelte';

	let { href, onClose }: { href: string; onClose: () => void } = $props();

	const MIN = 320;
	const MAX = 900;
	const width = $derived(uiPrefs.get<number>('pane.width', 480));
	/** The detail page with the shell left out (the app shows `?embed` pages bare). */
	const src = $derived(href + (href.includes('?') ? '&' : '?') + 'embed=1');
	let dragging = $state(false);

	function drag(event: PointerEvent) {
		const handle = event.currentTarget as HTMLElement;
		handle.setPointerCapture(event.pointerId);
		dragging = true;
		const right = (handle.parentElement as HTMLElement).getBoundingClientRect().right;
		const move = (e: PointerEvent) =>
			uiPrefs.set('pane.width', Math.round(Math.min(MAX, Math.max(MIN, right - e.clientX))));
		const stop = () => {
			dragging = false;
			handle.removeEventListener('pointermove', move);
			handle.removeEventListener('pointerup', stop);
		};
		handle.addEventListener('pointermove', move);
		handle.addEventListener('pointerup', stop);
	}

	function nudge(event: KeyboardEvent) {
		const step = event.key === 'ArrowLeft' ? 24 : event.key === 'ArrowRight' ? -24 : 0;
		if (step) uiPrefs.set('pane.width', Math.min(MAX, Math.max(MIN, width + step)));
	}
</script>

<!-- The page a list row opens, beside the list. The bar between them is dragged to resize. -->
<aside
	data-part="form.pane"
	class="relative ml-3 flex shrink-0 flex-col border-l border-border max-lg:fixed max-lg:inset-y-12 max-lg:right-0 max-lg:z-40 max-lg:ml-0 max-lg:bg-background max-lg:shadow-lg"
	style="width: min({width}px, 100vw)"
>
	<!-- A window splitter is focusable and takes arrow keys (WAI-ARIA); Svelte's rules do not know it. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
	<div
		role="separator"
		aria-orientation="vertical"
		aria-label="Resize the detail pane"
		aria-valuenow={width}
		aria-valuemin={MIN}
		aria-valuemax={MAX}
		tabindex="0"
		onpointerdown={drag}
		onkeydown={nudge}
		class="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize outline-none hover:bg-primary/30 focus-visible:bg-primary/40 {dragging
			? 'bg-primary/40'
			: ''}"
	></div>
	<div class="flex items-center justify-end gap-1 border-b border-border px-2 py-1">
		<!-- eslint-disable svelte/no-navigation-without-resolve -- the href already has the base path -->
		<Button size="sm" variant="ghost" href={href} title="Open on its own page">Open</Button>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
		<Button size="sm" variant="ghost" aria-label="Close the detail pane" onclick={onClose}>
			<AppIcon name="close" class="size-4" />
		</Button>
	</div>
	<iframe title="Detail" {src} class="min-h-0 w-full flex-1 border-0 bg-background {dragging ? 'pointer-events-none' : ''}"></iframe>
</aside>

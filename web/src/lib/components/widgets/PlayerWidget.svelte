<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';

	/**
	 * `<player source="lesson_player" report="watch_here" interval="15"/>`: plays a video or audio file and tells the plugin how far
	 * the person got. `source` is called with the page's values and answers `{ src, mime?, title?, position?, no_skipping?,
	 * enrolled?, link? }`. While it plays, and when it pauses or ends, `report` is called with the page's values and
	 * `{ seconds, ended }`, where `seconds` is the furthest point reached by playing (not by jumping). With `no_skipping` a jump
	 * ahead of that point is undone. The person's last place is where it starts. Nothing is reported unless `enrolled` is not
	 * false. Without `src`, a `link` (a web address) is shown instead: a video on another site cannot report.
	 */
	let { node }: { node: PageNode } = $props();

	interface Info {
		src: string;
		mime?: string;
		title?: string;
		position?: number;
		no_skipping?: boolean;
		enrolled?: boolean;
		link?: string;
	}

	const page = getPageState();
	const source = $derived(text(node.source));
	const report = $derived(text((node as Record<string, unknown>).report));
	const every = $derived(
		Math.min(120, Math.max(5, Number((node as Record<string, unknown>).interval) || 15))
	);

	let info = $state<Info | null>(null);
	let error = $state<string | null>(null);
	let video = $state<HTMLVideoElement | null>(null);
	let furthest = 0;
	let sent = -1;
	let timer: ReturnType<typeof setInterval> | undefined;
	let latest = 0;

	async function load() {
		if (!source || !page) return;
		const current = ++latest;
		const result = await page.call(source, page.args);
		if (current !== latest) return;
		if (!result.ok) {
			error = result.error;
			return;
		}
		error = null;
		const data = (result.data ?? {}) as Info;
		furthest = Math.max(0, Number(data.position) || 0);
		sent = Math.floor(furthest);
		info = data;
	}

	$effect(() => {
		void page?.plugin;
		void JSON.stringify(page?.args ?? {});
		untrack(() => void load());
	});

	async function send(ended = false) {
		if (!report || !page || !info || info.enrolled === false) return;
		const seconds = Math.floor(furthest);
		if (!ended && seconds === sent) return;
		sent = seconds;
		const r = await page.call(report, { ...page.args, seconds, ended });
		if (!r.ok) error = r.error;
		else if (ended) page.refresh();
	}

	function started() {
		if (video && furthest > 0 && video.currentTime < 1) video.currentTime = furthest;
	}

	function progressed() {
		if (!video) return;
		const at = video.currentTime;
		if (info?.no_skipping && at > furthest + 2) {
			video.currentTime = furthest;
			return;
		}
		furthest = Math.max(furthest, at);
	}

	function playing() {
		clearInterval(timer);
		timer = setInterval(() => void send(), every * 1000);
	}

	function stopped() {
		clearInterval(timer);
		void send();
	}

	function finished() {
		clearInterval(timer);
		if (video) furthest = Math.max(furthest, video.duration || 0);
		void send(true);
	}

	onDestroy(() => {
		clearInterval(timer);
		void send();
	});

	const safeLink = $derived(info?.link && /^https?:\/\//i.test(info.link) ? info.link : null);
</script>

<section class="space-y-2" data-part="player" aria-label={info?.title ?? 'Video'}>
	{#if error}<p class="text-xs text-destructive" role="alert">{error}</p>{/if}
	{#if info?.src}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video
			bind:this={video}
			class="max-h-[70vh] w-full rounded-md bg-black"
			controls
			preload="metadata"
			controlslist="nodownload"
			onloadedmetadata={started}
			ontimeupdate={progressed}
			onplay={playing}
			onpause={stopped}
			onended={finished}
		>
			<source src={info.src} type={info.mime || undefined} />
			Your browser cannot play this video.
		</video>
		{#if info.no_skipping}
			<p class="text-xs text-muted-foreground">Watch it through: skipping ahead is not counted.</p>
		{/if}
	{:else if safeLink}
		<p class="text-sm">
			This video plays on another site:
			<a class="underline" href={safeLink} target="_blank" rel="noopener noreferrer"
				>open the video</a
			>. Press <em>Mark as done</em> afterwards.
		</p>
	{/if}
</section>

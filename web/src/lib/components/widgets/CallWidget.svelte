<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { goto } from '$app/navigation';
	import type { PageNode } from '$lib/dsl/types';
	import { Button } from '$lib/components/ui/button';
	import { CallClient, signalingUrl } from '$lib/call/client';
	import { columnsFor, initials, orderTiles } from '$lib/call/layout';
	import { describeDevices, deviceOf, type DeviceLists } from '$lib/call/devices';
	import { navHref } from '$lib/components/layout/nav';
	import { topicName } from '$lib/pages/liveTopic.svelte';
	import { getPageState, interpolate, text } from '$lib/pages/pageContext.svelte';

	/**
	 * `<call room="meeting.{meeting}" leave="/meet"/>`: a video meeting (see `docs/architecture/meet_sfu.md`). First a
	 * check-your-devices step with a preview, then the room: a tile for each person, with microphone, camera and
	 * leave. `room` is a room the plugin declared with `[[sfu]]`; the plugin decides who may come in. `leave` is where
	 * the person goes when the call is over. The camera and microphone need a secure page (HTTPS, or localhost).
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const room = $derived(topicName(page, node.room));
	const leaveTo = $derived(text(node.leave));

	let preview = $state<MediaStream | null>(null);
	let previewError = $state<string | null>(null);
	// Joining waits until the camera and microphone have answered (allowed or not), or the call would start without them.
	let previewSettled = $state(false);
	let wantAudio = $state(true);
	let wantVideo = $state(true);
	let client = $state<CallClient | null>(null);
	// A counter the client's change callback bumps, so the template reads fresh fields from the plain class.
	let version = $state(0);
	let pinned = $state<string | null>(null);
	let previewEl = $state<HTMLVideoElement | null>(null);

	const secure = typeof window === 'undefined' || window.isSecureContext;

	// HD, so the camera has room for the three sizes of a simulcast send (a smaller picture is sent as fewer layers).
	const HD = { width: { ideal: 1280 }, height: { ideal: 720 } };

	async function openPreview() {
		try {
			await askForDevices();
		} finally {
			previewSettled = true;
		}
	}

	async function askForDevices() {
		previewError = null;
		if (!navigator.mediaDevices?.getUserMedia) {
			previewError = secure
				? 'This browser cannot use a camera or microphone.'
				: 'A camera and microphone need a secure page (HTTPS).';
			return;
		}
		try {
			preview = await navigator.mediaDevices.getUserMedia({ audio: true, video: HD });
		} catch {
			// Join anyway to listen and watch, the way every meeting app does.
			try {
				preview = await navigator.mediaDevices.getUserMedia({ audio: true });
				wantVideo = false;
				previewError = 'The camera could not be used; you will join with sound only.';
			} catch {
				preview = null;
				previewError =
					'The camera and microphone could not be used. You can still join to watch and listen.';
			}
		}
	}

	$effect(() => {
		void openPreview();
		return () => {
			// Leaving the page without joining must turn the camera light off.
			if (!client) for (const track of preview?.getTracks() ?? []) track.stop();
		};
	});

	$effect(() => {
		if (previewEl && preview) previewEl.srcObject = preview;
	});

	// --- devices: which microphone, camera and speaker. Listed once the browser has let the page see them.
	let devices = $state<DeviceLists>({ audioIn: [], videoIn: [], audioOut: [] });
	let micId = $state('');
	let camId = $state('');
	let speakerId = $state('');
	let showDevices = $state(false);
	let root = $state<HTMLElement | null>(null);

	async function listDevices() {
		try {
			devices = describeDevices(await navigator.mediaDevices.enumerateDevices());
		} catch {
			/* without a list the pickers stay empty */
		}
		const current = client?.stream ?? preview;
		micId = deviceOf(current?.getAudioTracks()[0]) || micId;
		camId = deviceOf(current?.getVideoTracks()[0]) || camId;
	}

	$effect(() => {
		if (!preview) return;
		void listDevices();
		const md = navigator.mediaDevices;
		md.addEventListener?.('devicechange', listDevices);
		return () => md.removeEventListener?.('devicechange', listDevices);
	});

	/** Use another microphone or camera: before joining it replaces the preview's, in the call the one being sent. */
	async function pickDevice(kind: 'audio' | 'video', id: string) {
		let fresh: MediaStream;
		try {
			fresh = await navigator.mediaDevices.getUserMedia(
				kind === 'audio'
					? { audio: { deviceId: { exact: id } } }
					: { video: { deviceId: { exact: id }, ...HD } }
			);
		} catch {
			previewError = 'That device could not be used.';
			return;
		}
		const track = (kind === 'audio' ? fresh.getAudioTracks() : fresh.getVideoTracks())[0];
		if (!track) return;
		previewError = null;
		if (client) await client.switchDevice(kind, track);
		else {
			const stream = preview ?? new MediaStream();
			const old = (kind === 'audio' ? stream.getAudioTracks() : stream.getVideoTracks())[0];
			track.enabled = kind === 'audio' ? wantAudio : wantVideo;
			if (old) {
				stream.removeTrack(old);
				old.stop();
			}
			stream.addTrack(track);
			preview = new MediaStream(stream.getTracks());
			if (previewEl) previewEl.srcObject = preview;
		}
		if (kind === 'audio') micId = id;
		else camId = id;
		version++;
	}

	// The speaker: where everyone else's voice plays (browsers without `setSinkId` keep the default).
	const canPickSpeaker =
		typeof HTMLMediaElement !== 'undefined' && 'setSinkId' in HTMLMediaElement.prototype;
	$effect(() => {
		void version;
		const id = speakerId;
		if (!id || !root) return;
		for (const video of root.querySelectorAll('video')) {
			void (video as HTMLVideoElement & { setSinkId?: (id: string) => Promise<void> })
				.setSinkId?.(id)
				.catch(() => undefined);
		}
	});

	function join() {
		if (!room) return;
		if (preview) {
			for (const track of preview.getAudioTracks()) track.enabled = wantAudio;
			for (const track of preview.getVideoTracks()) track.enabled = wantVideo;
		}
		client = new CallClient({
			url: signalingUrl(room, location),
			stream: preview,
			onChange: () => version++
		});
		client.start();
	}

	// `moderate="moderate_call"`: a plugin function the host's buttons call with the page's values and `action` (`mute`,
	// `remove`, `end`) and `user`; the plugin checks that the caller really is the host.
	const moderateFn = $derived(text(node.moderate));
	const isHost = $derived((void version, !!moderateFn && client?.role === 'host'));
	let problem = $state<string | null>(null);

	async function moderate(action: 'mute' | 'remove' | 'end', user = '') {
		if (!moderateFn || !page) return;
		const result = await page.call(moderateFn, { ...page.args, action, user });
		problem = result.ok ? null : result.error;
	}

	function leave() {
		client?.leave();
	}

	const phase = $derived((void version, client?.state ?? 'idle'));
	const tiles = $derived(
		(void version, client ? orderTiles(client.roster, client.you, client.speaking, pinned) : [])
	);
	// The client is a plain class: these read it through `version` so the buttons follow it.
	const micOn = $derived((void version, client?.audioOn ?? true));
	const camOn = $derived((void version, client?.videoOn ?? true));
	// Shared screens, one per person sharing. Your own is not shown back to you (it would mirror itself).
	const screens = $derived(
		(void version,
		client
			? client.roster
					.filter((p) => p.screen)
					.map((p) => ({
						user: p.user,
						name: p.name,
						you: p.user === client!.you,
						stream: p.user === client!.you ? null : (client!.screenOf(p.sid) ?? null)
					}))
			: [])
	);
	// Small tiles ask for the small picture: the strip beside a shared screen, or a crowded grid. A pinned person stays big.
	$effect(() => {
		void version;
		if (!client || phase !== 'live') return;
		const small = screens.length > 0 || tiles.length > 4;
		for (const tile of tiles) {
			if (!tile.you)
				client.want(tile.person.user, pinned === tile.person.user || !small ? 'high' : 'low');
		}
	});
	const sharingNow = $derived((void version, !!client?.sharing));
	const canShare = typeof navigator !== 'undefined' && !!navigator.mediaDevices?.getDisplayMedia;
	const columns = $derived(columnsFor(tiles.length));

	// A video element shows a stream: set it when the stream is known, once.
	function show(node: HTMLVideoElement, stream: MediaStream | null | undefined) {
		node.srcObject = stream ?? null;
		return {
			update(next: MediaStream | null | undefined) {
				if (node.srcObject !== (next ?? null)) node.srcObject = next ?? null;
			}
		};
	}

	function done() {
		if (leaveTo) void goto(navHref(interpolate(leaveTo, page?.vars ?? {})));
	}
</script>

{#snippet devicePicker()}
	<div class="grid gap-2 text-sm sm:grid-cols-3" data-part="call-devices">
		{#each [{ label: 'Microphone', list: devices.audioIn, value: micId, change: (id: string) => pickDevice('audio', id), show: true }, { label: 'Camera', list: devices.videoIn, value: camId, change: (id: string) => pickDevice('video', id), show: true }, { label: 'Speaker', list: devices.audioOut, value: speakerId, change: (id: string) => (speakerId = id), show: canPickSpeaker }] as picker (picker.label)}
			{#if picker.show && picker.list.length > 0}
				<label class="flex flex-col gap-1">
					<span class="text-xs text-muted-foreground">{picker.label}</span>
					<select
						class="h-8 w-full rounded-md border border-input bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
						aria-label={picker.label}
						value={picker.value}
						onchange={(event) => picker.change(event.currentTarget.value)}
					>
						{#each picker.list as device (device.id)}
							<option value={device.id}>{device.label}</option>
						{/each}
					</select>
				</label>
			{/if}
		{/each}
	</div>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col" data-part="call" bind:this={root}>
	{#if !room}
		<p class="p-4 text-sm text-muted-foreground">Choose a meeting to join.</p>
	{:else if phase === 'idle'}
		<div
			class="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 p-4"
			data-part="call-lobby"
		>
			<h2 class="type-heading">Ready to join?</h2>
			<div class="relative aspect-video overflow-hidden rounded-lg bg-muted">
				{#if preview && wantVideo && preview.getVideoTracks().length > 0}
					<video
						bind:this={previewEl}
						autoplay
						playsinline
						muted
						class="size-full -scale-x-100 object-cover"
						aria-label="Your camera"
					></video>
				{:else}
					<div class="grid size-full place-items-center text-sm text-muted-foreground">
						Camera off
					</div>
				{/if}
			</div>
			{#if previewError}<p class="text-sm text-muted-foreground" role="status">
					{previewError}
				</p>{/if}
			{@render devicePicker()}
			<div class="flex items-center justify-center gap-2">
				<Button
					type="button"
					variant={wantAudio ? 'outline' : 'destructive'}
					size="icon"
					aria-label={wantAudio ? 'Turn the microphone off' : 'Turn the microphone on'}
					aria-pressed={!wantAudio}
					disabled={!preview || preview.getAudioTracks().length === 0}
					onclick={() => (wantAudio = !wantAudio)}
				>
					{#if wantAudio}<AppIcon name="microphone" />{:else}<AppIcon name="microphone-off" />{/if}
				</Button>
				<Button
					type="button"
					variant={wantVideo ? 'outline' : 'destructive'}
					size="icon"
					aria-label={wantVideo ? 'Turn the camera off' : 'Turn the camera on'}
					aria-pressed={!wantVideo}
					disabled={!preview || preview.getVideoTracks().length === 0}
					onclick={() => (wantVideo = !wantVideo)}
				>
					{#if wantVideo}<AppIcon name="video" />{:else}<AppIcon name="video-off" />{/if}
				</Button>
				<Button type="button" onclick={join} disabled={!previewSettled}>Join now</Button>
			</div>
		</div>
	{:else if phase === 'joining'}
		<p class="p-6 text-center text-sm text-muted-foreground" role="status">Joining…</p>
	{:else if phase === 'failed' || phase === 'ended'}
		<div
			class="mx-auto flex max-w-md flex-1 flex-col items-center justify-center gap-3 p-6 text-center"
			role="status"
		>
			<p class="text-base font-medium">
				{phase === 'failed' ? 'Could not join' : 'The call has ended'}
			</p>
			<p class="text-sm text-muted-foreground">{client?.reason}</p>
			<div class="flex gap-2">
				{#if leaveTo}<Button type="button" onclick={done}>Back</Button>{/if}
				<Button
					type="button"
					variant="outline"
					onclick={() => {
						client = null;
						void openPreview();
					}}>Join again</Button
				>
			</div>
		</div>
	{:else if client}
		{#if screens.length > 0}
			<div
				class="grid min-h-0 flex-[3] gap-2 p-2"
				style="grid-template-columns: repeat({Math.min(
					screens.length,
					2
				)}, minmax(0, 1fr)); grid-auto-rows: minmax(0, 1fr);"
				data-part="call-screens"
			>
				{#each screens as screen (screen.user)}
					<div
						class="relative overflow-hidden rounded-lg bg-black"
						data-part="call-screen"
						data-user={screen.user}
					>
						{#if screen.stream}
							<video
								use:show={screen.stream}
								autoplay
								playsinline
								muted
								class="size-full object-contain"
								aria-label="{screen.name}'s screen"
							></video>
						{:else}
							<div
								class="grid size-full place-items-center p-4 text-center text-sm text-muted-foreground"
							>
								{screen.you ? 'You are sharing your screen' : `${screen.name} is sharing a screen…`}
							</div>
						{/if}
						<div
							class="absolute bottom-0 left-0 rounded-tr-md bg-background/80 px-2 py-0.5 text-xs"
						>
							{screen.you ? 'Your screen' : `${screen.name}'s screen`}
						</div>
					</div>
				{/each}
			</div>
		{/if}
		<div
			class="grid min-h-0 gap-2 p-2 {screens.length > 0 ? 'h-36 shrink-0' : 'flex-1'}"
			style="grid-template-columns: repeat({columns}, minmax(0, 1fr)); grid-auto-rows: minmax(0, 1fr);"
			data-part="call-grid"
			aria-label="People in the meeting"
		>
			{#each tiles as tile (tile.person.user)}
				{@const stream = tile.you ? client.stream : client.streams.get(tile.person.sid)}
				{@const speaking = client.speaking.has(tile.person.sid)}
				<div
					class="relative overflow-hidden rounded-lg bg-muted {speaking
						? 'ring-2 ring-primary'
						: ''}"
					data-part="call-tile"
					data-user={tile.person.user}
				>
					{#if tile.person.video && stream && stream.getVideoTracks().length > 0}
						<!-- A person's own tile is muted (their own voice) and mirrored, like every meeting app. -->
						<video
							use:show={stream}
							autoplay
							playsinline
							muted={tile.you}
							class="size-full object-cover {tile.you ? '-scale-x-100' : ''}"
							aria-label={tile.person.name}
						></video>
					{:else}
						<div class="grid size-full place-items-center">
							<span
								class="grid size-16 place-items-center rounded-full bg-secondary text-xl font-medium text-secondary-foreground"
								aria-hidden="true">{initials(tile.person.name)}</span
							>
						</div>
						<!-- Without a picture the voice still has to play. -->
						{#if !tile.you && stream}<video use:show={stream} autoplay playsinline class="hidden"
							></video>{/if}
					{/if}
					<div
						class="absolute bottom-0 left-0 flex max-w-full items-center gap-1 rounded-tr-md bg-background/80 px-2 py-0.5 text-xs"
					>
						{#if !tile.person.audio}<AppIcon
								name="microphone-off"
								class="size-3 shrink-0"
								aria-label="Muted"
							/>{/if}
						<span class="truncate">{tile.you ? `${tile.person.name} (you)` : tile.person.name}</span
						>
					</div>
					{#if isHost && !tile.you}
						<div class="absolute top-1 right-1 z-10 flex gap-1" data-part="call-moderation">
							{#if tile.person.audio}
								<Button
									type="button"
									size="icon-sm"
									variant="secondary"
									aria-label="Mute {tile.person.name}"
									onclick={() => moderate('mute', tile.person.user)}
									><AppIcon name="microphone-off" /></Button
								>
							{/if}
							<Button
								type="button"
								size="icon-sm"
								variant="secondary"
								aria-label="Remove {tile.person.name}"
								onclick={() => moderate('remove', tile.person.user)}
								><AppIcon name="close" /></Button
							>
						</div>
					{/if}
					<button
						type="button"
						class="absolute inset-0 cursor-pointer focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
						aria-label={pinned === tile.person.user
							? `Unpin ${tile.person.name}`
							: `Pin ${tile.person.name}`}
						onclick={() => (pinned = pinned === tile.person.user ? null : tile.person.user)}
					></button>
				</div>
			{/each}
		</div>
		{#if problem}<p class="px-2 text-center text-sm text-destructive" role="alert">
				{problem}
			</p>{/if}
		{#if showDevices}<div class="shrink-0 border-t border-border p-2">
				{@render devicePicker()}
			</div>{/if}
		<div
			class="flex shrink-0 items-center justify-center gap-2 border-t border-border p-2"
			data-part="call-controls"
		>
			<Button
				type="button"
				variant={micOn ? 'outline' : 'destructive'}
				size="icon"
				aria-label={micOn ? 'Mute microphone' : 'Unmute microphone'}
				aria-pressed={!micOn}
				disabled={!client.stream || client.stream.getAudioTracks().length === 0}
				onclick={() => client?.setMuted('audio', micOn)}
			>
				{#if micOn}<AppIcon name="microphone" />{:else}<AppIcon name="microphone-off" />{/if}
			</Button>
			<Button
				type="button"
				variant={camOn ? 'outline' : 'destructive'}
				size="icon"
				aria-label={camOn ? 'Turn camera off' : 'Turn camera on'}
				aria-pressed={!camOn}
				disabled={!client.stream || client.stream.getVideoTracks().length === 0}
				onclick={() => client?.setMuted('video', camOn)}
			>
				{#if camOn}<AppIcon name="video" />{:else}<AppIcon name="video-off" />{/if}
			</Button>
			<Button
				type="button"
				variant={showDevices ? 'default' : 'outline'}
				aria-label="Choose microphone, camera and speaker"
				aria-expanded={showDevices}
				onclick={() => (showDevices = !showDevices)}
			>
				<AppIcon name="settings" />
			</Button>
			{#if canShare}
				<Button
					type="button"
					variant={sharingNow ? 'default' : 'outline'}
					aria-label={sharingNow ? 'Stop sharing your screen' : 'Share your screen'}
					aria-pressed={sharingNow}
					onclick={() => (sharingNow ? client?.stopScreen() : void client?.shareScreen())}
				>
					<AppIcon name="expand-screen" />
				</Button>
			{/if}
			{#if isHost}
				<Button
					type="button"
					variant="outline"
					onclick={() => moderate('end')}
					aria-label="End the meeting for everyone">End for all</Button
				>
			{/if}
			<Button type="button" variant="destructive" onclick={leave} aria-label="Leave the meeting">
				<AppIcon name="phone-off" /> Leave
			</Button>
		</div>
	{/if}
</div>

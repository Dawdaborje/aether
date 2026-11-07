/**
 * The browser end of a video meeting (see `docs/architecture/meet_sfu.md`): the signaling socket and the two peer
 * connections. One connection carries what this person sends (the browser offers); the other carries everyone else's
 * tracks (the server offers). No UI here: a widget reads the fields and is told when they change.
 */

export type Role = 'host' | 'member';
export type CallState = 'idle' | 'joining' | 'live' | 'ended' | 'failed';

export interface Participant {
	/** The stream id of this person's tracks. */
	sid: string;
	user: string;
	name: string;
	role: Role;
	audio: boolean;
	video: boolean;
	screen: boolean;
	since: number;
}

export interface IceServer {
	urls: string[];
	username?: string;
	credential?: string;
}

/** What the client needs of a WebSocket, so a test can stand in for it. */
export interface SocketLike {
	send(data: string): void;
	close(): void;
	onopen: ((event: unknown) => void) | null;
	onmessage: ((event: { data: unknown }) => void) | null;
	onclose: ((event: unknown) => void) | null;
	onerror: ((event: unknown) => void) | null;
}

/** What the client needs of an RTCPeerConnection. */
export interface PeerLike {
	localDescription: { sdp: string } | null;
	ontrack: ((event: { streams: readonly MediaStream[]; track: MediaStreamTrack }) => void) | null;
	onicecandidate: ((event: { candidate: { candidate: string } | null }) => void) | null;
	addTrack(track: MediaStreamTrack, stream: MediaStream): RTCRtpSender;
	addTransceiver(track: MediaStreamTrack, init: RTCRtpTransceiverInit): RTCRtpTransceiver;
	getSenders(): RTCRtpSender[];
	getTransceivers(): RTCRtpTransceiver[];
	createOffer(): Promise<RTCSessionDescriptionInit>;
	createAnswer(): Promise<RTCSessionDescriptionInit>;
	setLocalDescription(description?: RTCSessionDescriptionInit): Promise<void>;
	setRemoteDescription(description: RTCSessionDescriptionInit): Promise<void>;
	addIceCandidate(candidate: RTCIceCandidateInit): Promise<void>;
	close(): void;
}

export interface CallOptions {
	/** The signaling socket address. */
	url: string;
	/** What this person sends: the stream from the check-your-devices step, or none to only listen. */
	stream?: MediaStream | null;
	socket?: (url: string) => SocketLike;
	peer?: (config: RTCConfiguration) => PeerLike;
	/** Asks the person which screen or window to share. */
	display?: () => Promise<MediaStream>;
	/** Send the camera at three sizes so the server can give each viewer the size that fits (default on). */
	simulcast?: boolean;
	/** Called after any field changed. */
	onChange?: () => void;
}

/** The camera's three layers: full size, half, and a quarter of each side. The server names them by `rid`. */
export const SIMULCAST: RTCRtpEncodingParameters[] = [
	{ rid: 'h', maxBitrate: 1_200_000 },
	{ rid: 'm', scaleResolutionDownBy: 2, maxBitrate: 400_000 },
	{ rid: 'l', scaleResolutionDownBy: 4, maxBitrate: 120_000 }
];

type Frame = { t: string } & Record<string, unknown>;

export class CallClient {
	state: CallState = 'idle';
	/** Why the call ended or failed, in words for the person. */
	reason = '';
	you = '';
	role: Role = 'member';
	roster: Participant[] = [];
	/** Everyone else's media, by the stream id (`sid`) of the person. */
	streams = new Map<string, MediaStream>();
	speaking = new Set<string>();
	/** What this person is sharing, while they share. */
	sharing: MediaStream | null = null;
	private screenSender: RTCRtpSender | null = null;
	private readonly wanted = new Map<string, string>();
	audioOn = true;
	videoOn = true;

	private socket: SocketLike | null = null;
	private pub: PeerLike | null = null;
	private sub: PeerLike | null = null;
	private queue: Promise<void> = Promise.resolve();
	private local: MediaStream | null;
	private readonly options: CallOptions;

	constructor(options: CallOptions) {
		this.options = options;
		this.local = options.stream ?? null;
		this.audioOn = !this.local || this.local.getAudioTracks().some((t) => t.enabled);
		this.videoOn = !this.local || this.local.getVideoTracks().some((t) => t.enabled);
	}

	/** The participant that is this person. */
	get me(): Participant | undefined {
		return this.roster.find((p) => p.user === this.you);
	}

	get stream(): MediaStream | null {
		return this.local;
	}

	start(): void {
		if (this.state !== 'idle') return;
		this.set('joining');
		const socket = (this.options.socket ?? ((url) => new WebSocket(url) as unknown as SocketLike))(this.options.url);
		this.socket = socket;
		socket.onmessage = (event) => {
			let frame: Frame;
			try {
				frame = JSON.parse(String(event.data)) as Frame;
			} catch {
				return;
			}
			// One frame at a time: handling an offer waits on the browser, and the next frame needs its result.
			this.queue = this.queue.then(() => this.handle(frame)).catch(() => undefined);
		};
		socket.onclose = () => {
			if (this.state === 'joining') this.finish('failed', this.reason || 'The meeting could not be reached.');
			else if (this.state === 'live') this.finish('ended', this.reason || 'You were disconnected.');
		};
		socket.onerror = () => {
			if (this.state === 'joining') this.reason = this.reason || 'The meeting could not be reached.';
		};
	}

	leave(): void {
		this.send({ op: 'leave' });
		this.finish('ended', 'You left the meeting.');
	}

	/**
	 * Send another microphone or camera from now on. The new track takes the old one's place on the connection (no new
	 * offer), keeps the muted state the person had, and the old track is let go so its light goes off.
	 */
	async switchDevice(kind: 'audio' | 'video', track: MediaStreamTrack): Promise<void> {
		const local = this.local;
		if (!local) return;
		const old = (kind === 'audio' ? local.getAudioTracks() : local.getVideoTracks())[0];
		track.enabled = kind === 'audio' ? this.audioOn : this.videoOn;
		const sender = this.pub?.getSenders().find((s) => s.track === old);
		if (sender) await sender.replaceTrack(track);
		if (old) {
			local.removeTrack(old);
			old.stop();
		}
		local.addTrack(track);
		this.changed();
	}

	/** The shared screen of a person, when they share one: a stream of its own, `<sid>-screen`. */
	screenOf(sid: string): MediaStream | undefined {
		return this.streams.get(`${sid}-screen`);
	}

	/** Share a screen or window. The server is told first, so it knows the next video line is a screen. */
	async shareScreen(): Promise<boolean> {
		if (this.state !== 'live' || !this.pub || this.sharing) return false;
		const ask = this.options.display ?? (() => navigator.mediaDevices.getDisplayMedia({ video: true }));
		let screen: MediaStream;
		try {
			screen = await ask();
		} catch {
			return false; // the person closed the picker
		}
		const video = screen.getVideoTracks()[0];
		if (!video) return false;
		this.sharing = screen;
		video.onended = () => this.stopScreen();
		this.send({ op: 'screen', sharing: true });
		this.screenSender = this.pub.addTrack(video, screen);
		await this.pub.setLocalDescription(await this.pub.createOffer());
		this.send({ op: 'offer', sdp: this.pub.localDescription?.sdp ?? '' });
		this.changed();
		return true;
	}

	stopScreen(): void {
		if (!this.sharing) return;
		for (const track of this.sharing.getTracks()) track.stop();
		this.sharing = null;
		this.send({ op: 'screen', sharing: false });
		// Close the screen's video line too, so a long call does not pile up empty ones.
		const sender = this.screenSender;
		this.screenSender = null;
		const line = sender ? this.pub?.getTransceivers().find((t) => t.sender === sender) : undefined;
		if (line && this.pub) {
			const pub = this.pub;
			line.stop();
			this.queue = this.queue
				.then(async () => {
					await pub.setLocalDescription(await pub.createOffer());
					this.send({ op: 'offer', sdp: pub.localDescription?.sdp ?? '' });
				})
				.catch(() => undefined);
		}
		this.changed();
	}

	/** Mute or unmute a kind of media; the others are told so their tile shows it. */
	setMuted(kind: 'audio' | 'video', muted: boolean): void {
		for (const track of kind === 'audio' ? this.local?.getAudioTracks() ?? [] : this.local?.getVideoTracks() ?? []) {
			track.enabled = !muted;
		}
		if (kind === 'audio') this.audioOn = !muted;
		else this.videoOn = !muted;
		this.send({ op: 'mute', kind, muted });
		this.changed();
	}

	/** Say how much of someone's video this person wants (a small tile asks for less). */
	want(user: string, quality: 'high' | 'low' | 'off'): void {
		if (this.wanted.get(user) === quality) return;
		this.wanted.set(user, quality);
		this.send({ op: 'want', user, quality });
	}

	private async handle(frame: Frame): Promise<void> {
		switch (frame.t) {
			case 'joined':
				return this.joined(frame);
			case 'offer': {
				if (!this.sub) return;
				await this.sub.setRemoteDescription({ type: 'offer', sdp: String(frame.sdp) });
				await this.sub.setLocalDescription(await this.sub.createAnswer());
				this.send({ op: 'answer', sdp: this.sub.localDescription?.sdp ?? '' });
				return;
			}
			case 'answer':
				await this.pub?.setRemoteDescription({ type: 'answer', sdp: String(frame.sdp) });
				return;
			case 'candidate': {
				const peer = frame.pc === 'sub' ? this.sub : this.pub;
				await peer?.addIceCandidate({ candidate: String(frame.candidate), sdpMid: '0', sdpMLineIndex: 0 }).catch(() => undefined);
				return;
			}
			case 'participant':
				return this.participant(frame.change as Record<string, Participant | { user: string }>);
			case 'speaking':
				this.speaking = new Set((frame.users as string[]) ?? []);
				return this.changed();
			case 'muted':
				// A host muted this person: the microphone or camera goes quiet here too (they can switch it on again).
				return this.setMuted(frame.kind === 'video' ? 'video' : 'audio', true);
			case 'removed':
				return this.finish('ended', String(frame.reason ?? 'You were removed from the meeting.'));
			case 'error':
				// A refusal to join arrives as an error before anything else; later ones are only worth showing.
				this.reason = String(frame.message ?? '');
				if (this.state === 'joining') this.finish('failed', this.reason);
				else this.changed();
		}
	}

	private async joined(frame: Frame): Promise<void> {
		this.you = String(frame.you);
		this.role = frame.role === 'host' ? 'host' : 'member';
		this.roster = (frame.participants as Participant[]) ?? [];
		const config: RTCConfiguration = { iceServers: ((frame.ice as IceServer[]) ?? []) as RTCIceServer[] };
		const make = this.options.peer ?? ((c) => new RTCPeerConnection(c) as unknown as PeerLike);
		this.pub = make(config);
		this.sub = make(config);
		this.pub.onicecandidate = (e) => e.candidate && this.send({ op: 'candidate', pc: 'pub', candidate: e.candidate.candidate });
		this.sub.onicecandidate = (e) => e.candidate && this.send({ op: 'candidate', pc: 'sub', candidate: e.candidate.candidate });
		this.sub.ontrack = (e) => {
			const stream = e.streams[0];
			if (!stream) return;
			this.streams = new Map(this.streams).set(stream.id, stream);
			this.changed();
		};
		this.set('live');
		if (this.local && this.local.getTracks().length > 0) {
			for (const track of this.local.getTracks()) {
				if (track.kind === 'video' && this.options.simulcast !== false) {
					this.pub.addTransceiver(track, { direction: 'sendonly', streams: [this.local], sendEncodings: SIMULCAST });
				} else this.pub.addTrack(track, this.local);
			}
			await this.pub.setLocalDescription(await this.pub.createOffer());
			this.send({ op: 'offer', sdp: this.pub.localDescription?.sdp ?? '' });
		}
	}

	private participant(change: Record<string, Participant | { user: string }>): void {
		const joined = change.join as Participant | undefined;
		const updated = change.update as Participant | undefined;
		const left = change.leave as { user: string } | undefined;
		if (joined || updated) {
			const who = (joined ?? updated) as Participant;
			this.roster = [...this.roster.filter((p) => p.user !== who.user), who];
		}
		// Someone stopped sharing: their screen's stream goes.
		const stopped = (joined ?? updated) && !(joined ?? updated)!.screen ? (joined ?? updated)! : undefined;
		if (stopped && this.streams.has(`${stopped.sid}-screen`)) {
			const next = new Map(this.streams);
			next.delete(`${stopped.sid}-screen`);
			this.streams = next;
		}
		if (left) {
			const gone = this.roster.find((p) => p.user === left.user);
			this.roster = this.roster.filter((p) => p.user !== left.user);
			if (gone && (this.streams.has(gone.sid) || this.streams.has(`${gone.sid}-screen`))) {
				const next = new Map(this.streams);
				next.delete(gone.sid);
				next.delete(`${gone.sid}-screen`);
				this.streams = next;
			}
		}
		this.changed();
	}

	private send(frame: Record<string, unknown>): void {
		try {
			this.socket?.send(JSON.stringify(frame));
		} catch {
			/* a closed socket ends the call through onclose */
		}
	}

	private finish(state: CallState, reason: string): void {
		if (this.state === 'ended' || this.state === 'failed') return;
		this.reason = reason;
		for (const track of this.sharing?.getTracks() ?? []) track.stop();
		this.sharing = null;
		this.pub?.close();
		this.sub?.close();
		this.pub = this.sub = null;
		for (const track of this.local?.getTracks() ?? []) track.stop();
		this.streams = new Map();
		try {
			this.socket?.close();
		} catch {
			/* already closed */
		}
		this.set(state);
	}

	private set(state: CallState): void {
		this.state = state;
		this.changed();
	}

	private changed(): void {
		this.options.onChange?.();
	}
}

/** The signaling address for a room (`plugin:room`), on this site. */
export function signalingUrl(room: string, org: string | null | undefined, here: { protocol: string; host: string }): string {
	const scheme = here.protocol === 'https:' ? 'wss' : 'ws';
	return `${scheme}://${here.host}/api/ui/sfu/${encodeURIComponent(room)}${org ? `?org=${encodeURIComponent(org)}` : ''}`;
}

/**
 * The browser end of the realtime WebSocket (see `docs/architecture/realtime.md`).
 *
 * One socket per page load, opened when the first topic is joined and closed shortly after the last one is left.
 * Joined topics are counted, so two widgets on the same topic share one subscription. After a dropped connection
 * the client reconnects with exponential backoff and joins again from the last sequence number it saw, so a short
 * outage loses nothing the server still remembers; for a longer one the server says `resync` and the handler
 * reads from the database.
 *
 * This file does not depend on Svelte or on `window`, so it can be tested with a fake socket.
 */

export interface MessageFrame {
	topic: string;
	seq: number;
	event: string;
	payload: unknown;
}
export interface EventFrame {
	topic: string;
	kind: string;
	user: string;
	payload: unknown;
}
export interface PresenceFrame {
	topic: string;
	join?: string;
	leave?: string;
	/** This person is connected but not looking at the topic (another page or app, a hidden tab). */
	idle?: string;
	/** This person is looking at it again. */
	active?: string;
}

export interface TopicHandlers {
	/** A published message (replayed ones too, in order, each once). */
	onMessage?(frame: MessageFrame): void;
	/** A client event from someone else (typing). */
	onEvent?(frame: EventFrame): void;
	onPresence?(frame: PresenceFrame): void;
	/** The topic was joined; `presence` lists who is connected, when the topic tracks it. */
	onJoined?(info: { seq: number; presence: string[]; idle: string[] }): void;
	/** Messages were missed beyond what the server keeps: read them from the database. */
	onResync?(): void;
	/** The server refused the join, or ended the membership. */
	onDenied?(reason: string): void;
}

export interface SocketLike {
	readyState: number;
	send(data: string): void;
	close(): void;
	onopen: (() => void) | null;
	onmessage: ((event: { data: unknown }) => void) | null;
	onclose: (() => void) | null;
	onerror: (() => void) | null;
}

export interface ClientOptions {
	url(): string;
	socket?(url: string): SocketLike;
	/** Whether the page is in front of the person; a hidden tab counts as idle. Default: always. */
	visible?(): boolean;
	/** Pauses, replaceable in tests. */
	setTimer?(fn: () => void, ms: number): unknown;
	clearTimer?(handle: unknown): void;
}

const OPEN = 1;
const MAX_BACKOFF = 30_000;
const LINGER_MS = 5_000;
const PING_MS = 25_000;

interface Entry {
	handlers: Set<TopicHandlers>;
	/** The newest sequence number seen; sent as `since` when joining again. */
	seq: number | null;
	joined: boolean;
	/** Who is connected, as last told (the join answer, then each join and leave). */
	presence: Set<string>;
	/** Of those, who is not looking at the topic. */
	idle: Set<string>;
	/** Stay on the topic when the last widget leaves, as idle (a person stays "here" while they use another page). */
	keep: boolean;
	/** What the server was last told about this connection, to say it only when it changes. */
	reported: boolean | null;
}

export interface SubscribeOptions {
	/**
	 * Keep the topic for the whole session: when the last widget leaves, the person stays on it as idle instead of
	 * leaving, and the socket stays open. For presence ("online") topics, where leaving the page must not look like
	 * leaving the app.
	 */
	keep?: boolean;
}

export class RealtimeClient {
	private socket: SocketLike | null = null;
	private entries = new Map<string, Entry>();
	private backoff = 1000;
	private retry: unknown = null;
	private linger: unknown = null;
	private ping: unknown = null;
	private statusListeners = new Set<(connected: boolean) => void>();
	private _connected = false;
	private shown = true;

	constructor(private readonly options: ClientOptions) {}

	get connected(): boolean {
		return this._connected;
	}

	/** Tell `listener` whenever the socket opens or closes. Returns the way to stop. */
	onStatus(listener: (connected: boolean) => void): () => void {
		this.statusListeners.add(listener);
		return () => this.statusListeners.delete(listener);
	}

	/** Join a topic (`<plugin>:<name>`). Returns the way to leave it. */
	subscribe(topic: string, handlers: TopicHandlers, options: SubscribeOptions = {}): () => void {
		let entry = this.entries.get(topic);
		const fresh = !entry;
		if (!entry) {
			entry = {
				handlers: new Set(),
				seq: null,
				joined: false,
				presence: new Set(),
				idle: new Set(),
				keep: false,
				reported: null
			};
			this.entries.set(topic, entry);
		}
		if (options.keep) entry.keep = true;
		entry.handlers.add(handlers);
		this.cancelLinger();
		if (fresh || (entry.handlers.size === 1 && !entry.joined)) this.join(topic);
		else if (entry.joined) {
			handlers.onJoined?.({ seq: entry.seq ?? 0, presence: [...entry.presence], idle: [...entry.idle] });
			this.report(topic, entry);
		}
		this.ensureOpen();
		return () => this.unsubscribe(topic, handlers);
	}

	/**
	 * Stay on a presence topic for the whole session without looking at it: the person counts as there, and as idle
	 * until a widget subscribes to the same topic (then active). Returns the way to let go.
	 */
	hold(topic: string): () => void {
		let entry = this.entries.get(topic);
		const fresh = !entry;
		if (!entry) {
			entry = { handlers: new Set(), seq: null, joined: false, presence: new Set(), idle: new Set(), keep: true, reported: null };
			this.entries.set(topic, entry);
		}
		entry.keep = true;
		this.cancelLinger();
		if (fresh) this.join(topic);
		this.ensureOpen();
		return () => {
			const current = this.entries.get(topic);
			if (!current) return;
			current.keep = false;
			if (current.handlers.size > 0) return;
			this.entries.delete(topic);
			this.write({ op: 'unsub', topic });
			if (this.entries.size === 0) {
				this.cancelLinger();
				this.linger = this.timer(() => this.stop(), LINGER_MS);
			}
		};
	}

	/** The page came to the front or went behind: a topic kept for presence follows (hidden means idle). */
	setVisible(visible: boolean): void {
		this.shown = visible;
		for (const [topic, entry] of this.entries) this.report(topic, entry);
	}

	/**
	 * Tell the server whether this connection is looking at a topic, when that changed: not looking means the page is in
	 * the background, or (for a kept topic) no widget shows it. A topic starts as looked at.
	 */
	private report(topic: string, entry: Entry): void {
		if (!entry.joined) return;
		const hidden = !(this.options.visible?.() ?? this.shown);
		const idle = hidden || (entry.keep && entry.handlers.size === 0);
		if ((entry.reported ?? false) === idle) {
			entry.reported = idle;
			return;
		}
		entry.reported = idle;
		this.write({ op: 'idle', topic, idle });
	}

	/** Send a client event (typing…) to the others on a topic. Dropped when not connected. */
	sendEvent(topic: string, kind: string, payload: unknown = null): boolean {
		if (!this.socket || this.socket.readyState !== OPEN || !this.entries.get(topic)?.joined)
			return false;
		this.socket.send(JSON.stringify({ op: 'event', topic, kind, payload }));
		return true;
	}

	/** Close the socket and forget every topic (sign-out, organization change). */
	close(): void {
		this.cancelLinger();
		this.entries.clear();
		this.stop();
	}

	private timer(fn: () => void, ms: number): unknown {
		return (this.options.setTimer ?? ((f, m) => setTimeout(f, m)))(fn, ms);
	}
	private untimer(handle: unknown): void {
		(this.options.clearTimer ?? ((h) => clearTimeout(h as ReturnType<typeof setTimeout>)))(handle);
	}

	private unsubscribe(topic: string, handlers: TopicHandlers): void {
		const entry = this.entries.get(topic);
		if (!entry) return;
		entry.handlers.delete(handlers);
		if (entry.handlers.size > 0) return;
		if (entry.keep) {
			// Stay on the topic, idle, and keep the socket open: the person is still here, on another page.
			this.report(topic, entry);
			return;
		}
		this.entries.delete(topic);
		this.write({ op: 'unsub', topic });
		if (this.entries.size === 0) {
			this.cancelLinger();
			this.linger = this.timer(() => this.stop(), LINGER_MS);
		}
	}

	private cancelLinger(): void {
		if (this.linger !== null) this.untimer(this.linger);
		this.linger = null;
	}

	private write(frame: Record<string, unknown>): void {
		if (this.socket && this.socket.readyState === OPEN) this.socket.send(JSON.stringify(frame));
	}

	private join(topic: string): void {
		const since = this.entries.get(topic)?.seq;
		this.write(since == null ? { op: 'sub', topic } : { op: 'sub', topic, since });
	}

	private setConnected(value: boolean): void {
		if (this._connected === value) return;
		this._connected = value;
		for (const listener of this.statusListeners) listener(value);
	}

	private ensureOpen(): void {
		if (this.socket || this.retry !== null || this.entries.size === 0) return;
		this.open();
	}

	private open(): void {
		const make =
			this.options.socket ?? ((url: string) => new WebSocket(url) as unknown as SocketLike);
		let socket: SocketLike;
		try {
			socket = make(this.options.url());
		} catch {
			this.scheduleRetry();
			return;
		}
		this.socket = socket;
		socket.onopen = () => {
			this.backoff = 1000;
			this.setConnected(true);
			for (const topic of this.entries.keys()) this.join(topic);
			this.schedulePing();
		};
		socket.onmessage = (event) => this.receive(event.data);
		const ended = () => {
			if (this.socket !== socket) return;
			this.socket = null;
			this.setConnected(false);
			if (this.ping !== null) this.untimer(this.ping);
			this.ping = null;
			for (const entry of this.entries.values()) {
				entry.joined = false;
				entry.reported = null;
			}
			if (this.entries.size > 0) this.scheduleRetry();
		};
		socket.onclose = ended;
		socket.onerror = () => {
			/* `onclose` follows */
		};
	}

	private schedulePing(): void {
		this.ping = this.timer(() => {
			this.write({ op: 'ping' });
			this.schedulePing();
		}, PING_MS);
	}

	private scheduleRetry(): void {
		if (this.retry !== null) return;
		const wait = this.backoff;
		this.backoff = Math.min(this.backoff * 2, MAX_BACKOFF);
		this.retry = this.timer(() => {
			this.retry = null;
			this.ensureOpen();
		}, wait);
	}

	private stop(): void {
		if (this.retry !== null) this.untimer(this.retry);
		this.retry = null;
		if (this.ping !== null) this.untimer(this.ping);
		this.ping = null;
		const socket = this.socket;
		this.socket = null;
		if (socket) {
			socket.onclose = null;
			socket.close();
		}
		this.setConnected(false);
	}

	private receive(data: unknown): void {
		if (typeof data !== 'string') return;
		let frame: Record<string, unknown>;
		try {
			frame = JSON.parse(data) as Record<string, unknown>;
		} catch {
			return;
		}
		const topic = typeof frame.topic === 'string' ? frame.topic : '';
		const entry = this.entries.get(topic);
		switch (frame.t) {
			case 'sub': {
				if (!entry) return;
				if (frame.ok === true) {
					entry.joined = true;
					const seq = Number(frame.seq) || 0;
					// A server that restarted starts counting again; take its number.
					if (entry.seq === null || entry.seq > seq) entry.seq = seq;
					const presence = Array.isArray(frame.presence) ? (frame.presence as string[]) : [];
					entry.presence = new Set(presence);
					const idle = Array.isArray(frame.idle) ? (frame.idle as string[]) : [];
					entry.idle = new Set(idle);
					entry.reported = null;
					for (const h of entry.handlers) h.onJoined?.({ seq, presence, idle });
					this.report(topic, entry);
				} else {
					entry.joined = false;
					const reason = typeof frame.reason === 'string' ? frame.reason : 'not allowed';
					for (const h of entry.handlers) h.onDenied?.(reason);
				}
				return;
			}
			case 'msg': {
				if (!entry) return;
				const seq = Number(frame.seq);
				// A replay that overlaps what was already delivered is skipped.
				if (entry.seq !== null && seq <= entry.seq) return;
				entry.seq = seq;
				const message: MessageFrame = {
					topic,
					seq,
					event: String(frame.event ?? ''),
					payload: frame.payload
				};
				for (const h of entry.handlers) h.onMessage?.(message);
				return;
			}
			case 'event':
				if (entry)
					for (const h of entry.handlers)
						h.onEvent?.({
							topic,
							kind: String(frame.kind ?? ''),
							user: String(frame.user ?? ''),
							payload: frame.payload
						});
				return;
			case 'presence':
				if (entry) {
					const text = (value: unknown) => (typeof value === 'string' ? value : undefined);
					const join = text(frame.join);
					const leave = text(frame.leave);
					const idle = text(frame.idle);
					const active = text(frame.active);
					if (join) entry.presence.add(join);
					if (leave) {
						entry.presence.delete(leave);
						entry.idle.delete(leave);
					}
					if (idle) entry.idle.add(idle);
					if (active) entry.idle.delete(active);
					for (const h of entry.handlers) h.onPresence?.({ topic, join, leave, idle, active });
				}
				return;
			case 'resync':
				if (entry) {
					// Whatever was missed is read from the database; counting starts from here.
					entry.seq = null;
					for (const h of entry.handlers) h.onResync?.();
				}
				return;
			case 'dropped':
				if (entry) {
					entry.joined = false;
					this.entries.delete(topic);
					for (const h of entry.handlers) h.onDenied?.('removed');
				}
				return;
			case 'bye':
				// The socket closes next; `onclose` reconnects (right away after a planned end).
				this.backoff = 500;
				return;
			default:
		}
	}
}

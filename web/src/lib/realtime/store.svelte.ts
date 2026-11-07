import { RealtimeClient, type SubscribeOptions, type TopicHandlers } from './client';

/** The WebSocket address of this site. */
function address(): string {
	const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
	return `${scheme}://${location.host}/api/ui/realtime`;
}

let client: RealtimeClient | null = null;

/** The page's one realtime client; made on first use, so pages that never use it open no socket. */
export function realtime(): RealtimeClient {
	if (!client) {
		client = new RealtimeClient({ url: address, visible: () => document.visibilityState !== 'hidden' });
		// A tab in the background is idle for the topics that track presence.
		document.addEventListener('visibilitychange', () => client?.setVisible(document.visibilityState !== 'hidden'));
	}
	return client;
}

/** Forget every topic and close the socket: on sign-out and when the organization changes. */
export function resetRealtime(): void {
	client?.close();
	client = null;
}

/** Join a topic until the returned function is called. */
export function subscribeTopic(topic: string, handlers: TopicHandlers, options?: SubscribeOptions): () => void {
	return realtime().subscribe(topic, handlers, options);
}

/** Stay on a presence topic for the whole session (idle while no widget looks at it); see `RealtimeClient.hold`. */
export function holdTopic(topic: string): () => void {
	return realtime().hold(topic);
}

/**
 * A reactive view of one topic for a component: who is connected, who is typing, and the connection state.
 * Call `start()` when the component mounts (inside `$effect`) and use the returned function to stop.
 */
export class TopicState {
	present = $state<string[]>([]);
	typing = $state<string[]>([]);
	joined = $state(false);
	denied = $state<string | null>(null);
	private timers = new Map<string, ReturnType<typeof setTimeout>>();
	private sender = 0;

	constructor(
		private readonly topic: string,
		private readonly extra: TopicHandlers = {}
	) {}

	start(): () => void {
		const off = subscribeTopic(this.topic, {
			onJoined: (info) => {
				this.joined = true;
				this.denied = null;
				this.present = info.presence;
				this.extra.onJoined?.(info);
			},
			onDenied: (reason) => {
				this.joined = false;
				this.denied = reason;
				this.extra.onDenied?.(reason);
			},
			onMessage: (m) => this.extra.onMessage?.(m),
			onResync: () => this.extra.onResync?.(),
			onPresence: (p) => {
				if (p.join && !this.present.includes(p.join)) this.present = [...this.present, p.join];
				if (p.leave) this.present = this.present.filter((u) => u !== p.leave);
				this.extra.onPresence?.(p);
			},
			onEvent: (e) => {
				if (e.kind === 'typing') this.markTyping(e.user);
				this.extra.onEvent?.(e);
			}
		});
		return () => {
			off();
			for (const t of this.timers.values()) clearTimeout(t);
			this.timers.clear();
			this.joined = false;
		};
	}

	/** Tell the others this person is typing; at most once every two seconds. */
	typingNow(): void {
		const now = Date.now();
		if (now - this.sender < 2000) return;
		this.sender = now;
		realtime().sendEvent(this.topic, 'typing');
	}

	private markTyping(user: string): void {
		if (!this.typing.includes(user)) this.typing = [...this.typing, user];
		clearTimeout(this.timers.get(user));
		// A typing signal that is not repeated fades out.
		this.timers.set(
			user,
			setTimeout(() => {
				this.typing = this.typing.filter((u) => u !== user);
				this.timers.delete(user);
			}, 4000)
		);
	}
}

import { apiFetch } from '$lib/api/client';
import { readEventStream } from './sse';

export type NotificationLevel = 'info' | 'success' | 'warning' | 'error';

export interface AppNotification {
	id: string;
	source: string;
	level: NotificationLevel;
	title: string;
	body: string | null;
	link: string | null;
	payload: Record<string, unknown> | null;
	created_at: string;
	read: boolean;
}

/** A transient plugin event (`events::emit`); shown to members, never stored. */
export interface PluginEvent {
	plugin: string;
	event: string;
	payload: unknown;
}

const STREAM = '/api/ui/notifications/stream';
const TOAST_MS = 7000;
const MAX_ITEMS = 100;
const MAX_BACKOFF_MS = 30_000;

/** Answers that retrying will not change until something else does (login, organization). */
const FINAL = new Set([401, 403, 404, 409]);

/**
 * Notifications for whoever is using the app, members and anonymous visitors alike.
 *
 * One long-lived server-sent-events connection delivers new ones; the list and the
 * unread count come from plain requests. If the connection drops it comes back with
 * `Last-Event-ID`, so what happened meanwhile is replayed.
 */
class NotificationsStore {
	items = $state<AppNotification[]>([]);
	unread = $state(0);
	toasts = $state<AppNotification[]>([]);
	connected = $state(false);

	private abort: AbortController | null = null;
	private lastId: string | null = null;
	private listeners = new Set<(event: PluginEvent) => void>();

	/** Open the stream (once); loads what is already there. */
	start(): void {
		if (this.abort) return;
		const abort = new AbortController();
		this.abort = abort;
		void this.refresh();
		void this.run(abort);
	}

	stop(): void {
		this.abort?.abort();
		this.abort = null;
		this.connected = false;
	}

	/** Start over, e.g. after the organization changed: nothing carries across. */
	restart(): void {
		this.stop();
		this.items = [];
		this.unread = 0;
		this.toasts = [];
		this.lastId = null;
		this.start();
	}

	/** Be told about plugin events. Returns the function that stops listening. */
	onEvent(listener: (event: PluginEvent) => void): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	async refresh(): Promise<void> {
		try {
			const res = await apiFetch('/api/ui/notifications?limit=50');
			if (!res.ok) return;
			const body = (await res.json()) as { notifications: AppNotification[]; unread: number };
			this.items = body.notifications;
			this.unread = body.unread;
			// Anything newer than the list was delivered by the stream already.
			this.lastId = body.notifications[0]?.id ?? this.lastId;
		} catch {
			// Offline: the stream's reconnect will refresh.
		}
	}

	async markRead(ids?: string[]): Promise<void> {
		const targets = ids ?? this.items.filter((item) => !item.read).map((item) => item.id);
		if (targets.length === 0) return;
		const before = this.items;
		this.items = this.items.map((item) => (targets.includes(item.id) ? { ...item, read: true } : item));
		this.unread = this.items.filter((item) => !item.read).length;
		try {
			const res = await apiFetch('/api/ui/notifications/read', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(ids ? { ids } : {})
			});
			if (!res.ok) throw new Error(String(res.status));
		} catch {
			// Put it back: the server did not record it.
			this.items = before;
			this.unread = before.filter((item) => !item.read).length;
		}
	}

	dismissToast(id: string): void {
		this.toasts = this.toasts.filter((toast) => toast.id !== id);
	}

	private receive(notification: AppNotification): void {
		if (this.items.some((item) => item.id === notification.id)) return;
		const fresh = { ...notification, read: false };
		this.items = [fresh, ...this.items].slice(0, MAX_ITEMS);
		this.unread += 1;
		this.toasts = [...this.toasts, fresh].slice(-4);
		setTimeout(() => this.dismissToast(fresh.id), TOAST_MS);
	}

	private async run(abort: AbortController): Promise<void> {
		let backoff = 1000;
		while (!abort.signal.aborted) {
			try {
				const headers = new Headers({ Accept: 'text/event-stream' });
				if (this.lastId) headers.set('Last-Event-ID', this.lastId);
				const res = await apiFetch(STREAM, { headers, signal: abort.signal });
				if (FINAL.has(res.status)) {
					// Not something to retry now; a new organization or a login restarts us.
					this.connected = false;
					return;
				}
				if (!res.ok || !res.body) throw new Error(`stream ${res.status}`);
				this.connected = true;
				backoff = 1000;
				// Anything stored between the first list and this connection is picked up here.
				void this.refresh();
				await readEventStream(res.body, (message) => {
					if (message.id) this.lastId = message.id;
					if (message.event === 'notification') {
						this.receive(JSON.parse(message.data) as AppNotification);
					} else if (message.event === 'resync') {
						void this.refresh();
					} else if (message.event === 'event') {
						const event = JSON.parse(message.data) as PluginEvent;
						for (const listener of this.listeners) listener(event);
					}
					// "reconnect": the server's end-of-life for this stream; the loop reopens it.
				});
			} catch {
				if (abort.signal.aborted) return;
			}
			this.connected = false;
			if (abort.signal.aborted) return;
			await new Promise((resolve) => setTimeout(resolve, backoff));
			backoff = Math.min(backoff * 2, MAX_BACKOFF_MS);
		}
	}
}

export const notificationsStore = new NotificationsStore();

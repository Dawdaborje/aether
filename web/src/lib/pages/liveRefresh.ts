import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';

/** Payload of the kernel's transient `aether` / `data.changed` SSE event. */
export interface DataChanged {
	model: string;
	op: string;
	id?: string | null;
}

const DEBOUNCE_MS = 200;

/**
 * Listen for kernel `data.changed` events. When `shouldRefresh` accepts the
 * payload, call `onRefresh` after a short debounce (bursts of writes → one reload).
 * Returns an unsubscribe function.
 */
export function subscribeDataChanged(
	shouldRefresh: (payload: DataChanged) => boolean,
	onRefresh: () => void,
	debounceMs = DEBOUNCE_MS
): () => void {
	let timer: ReturnType<typeof setTimeout> | null = null;
	const stop = notificationsStore.onEvent((event) => {
		if (event.plugin !== 'aether' || event.event !== 'data.changed') return;
		const raw = event.payload;
		if (!raw || typeof raw !== 'object') return;
		const model = String((raw as { model?: unknown }).model ?? '').trim();
		if (!model) return;
		const payload: DataChanged = {
			model,
			op: String((raw as { op?: unknown }).op ?? ''),
			id: (raw as { id?: string | null }).id ?? null
		};
		if (!shouldRefresh(payload)) return;
		if (timer) clearTimeout(timer);
		timer = setTimeout(() => {
			timer = null;
			onRefresh();
		}, debounceMs);
	});
	return () => {
		if (timer) clearTimeout(timer);
		stop();
	};
}

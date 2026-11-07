import { holdTopic, resetRealtime } from './store.svelte';

/** The kernel's presence topic: every signed-in page of the organization is on it (see `facets/core/src/realtime/ws.rs`). */
export const ONLINE_TOPIC = 'kernel:online';

let release: (() => void) | null = null;

/**
 * Count this person as online for as long as they have a signed-in page open, whichever app it is on: idle when no
 * widget looks at the topic (the chat's list of people does), active when one does.
 */
export function startSessionPresence(): void {
	if (release) return;
	release = holdTopic(ONLINE_TOPIC);
}

/** Leave and close the socket: on sign-out, and before another organization's session starts. */
export function stopSessionPresence(): void {
	release?.();
	release = null;
	resetRealtime();
}

/**
 * The browser's localStorage, or null where there is none (server rendering, tests in a
 * plain Node process, blocked storage). Touching the `localStorage` global in Node
 * prints an ExperimentalWarning, so it is only read once a `window` exists.
 */
export function browserStorage(): Storage | null {
	if (typeof window === 'undefined') return null;
	try {
		return window.localStorage;
	} catch {
		return null;
	}
}

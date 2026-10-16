import { classifyStatus, pageError, type ErrorKind } from '$lib/pages/pageContext.svelte';

/**
 * Show a failure in the shared error modal, the way a failed plugin call is shown, and return
 * its text so a page can still keep a state of its own. Use this for any request a page makes
 * that fails (loading a list, saving, running an action): inline red text is for field
 * validation only.
 *
 * `error` is what was caught. A message that ends with a status (`… could not be loaded (502).`,
 * which is how the API helpers word a failure) is given the matching title (server error,
 * permission error, …).
 */
export function reportError(error: unknown, fallback: string, where = 'aether'): string {
	const message = error instanceof Error && error.message ? error.message : fallback;
	const status = Number(/\((\d{3})\)\.?\s*$/.exec(message)?.[1] ?? 0);
	const kind: ErrorKind = status ? classifyStatus(status) : 'validation';
	pageError.current = { kind, status, message, plugin: where, fn: '' };
	return message;
}

/**
 * What the open page tells the shell, so blocks outside the page (`save-bar`, `action-bar`) can
 * show it: a form with unsaved changes, rows selected in a list. Pages publish and clear it as
 * they open and close.
 */
class PageState {
	/** A form holding changes: save or discard them. */
	form = $state<{ save: () => void; discard: () => void; saving: boolean } | null>(null);
	/** Rows selected in a list, and how to clear the selection. */
	selection = $state<{ count: number; clear: () => void } | null>(null);
}

export const pageState = new PageState();

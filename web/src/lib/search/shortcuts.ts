/**
 * Keyboard shortcuts for palette entries. A shortcut is written `mod`, `alt` and `shift` (in
 * that order) joined by `+`, then a letter or digit: `alt+n`, `mod+shift+e`. `mod` is ⌘ on a
 * Mac and Ctrl elsewhere. The server normalises what plugins write the same way.
 */

export const isMacPlatform = (): boolean =>
	typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);

type KeyLike = Pick<KeyboardEvent, 'code' | 'altKey' | 'shiftKey' | 'ctrlKey' | 'metaKey'>;

/**
 * The shortcut a key press makes, or `null` when it is not one (no `mod` or `alt`, or not a
 * letter or digit). Reads `code`, not `key`, because Alt/Option changes the character typed.
 */
export function shortcutOf(event: KeyLike, mac = isMacPlatform()): string | null {
	const key = /^Key([A-Z])$/.exec(event.code)?.[1] ?? /^Digit([0-9])$/.exec(event.code)?.[1];
	if (!key) return null;
	const mod = mac ? event.metaKey : event.ctrlKey;
	// The other platform key is not part of any shortcut.
	if (mac ? event.ctrlKey : event.metaKey) return null;
	if (!mod && !event.altKey) return null;
	return [mod && 'mod', event.altKey && 'alt', event.shiftKey && 'shift', key.toLowerCase()]
		.filter(Boolean)
		.join('+');
}

/** How a shortcut is shown: `⌥⇧N` on a Mac, `Alt+Shift+N` elsewhere. */
export function formatShortcut(shortcut: string, mac = isMacPlatform()): string {
	const parts = shortcut.split('+');
	const key = (parts.pop() ?? '').toUpperCase();
	const names = mac
		? { mod: '⌘', alt: '⌥', shift: '⇧' }
		: { mod: 'Ctrl', alt: 'Alt', shift: 'Shift' };
	const mods = parts.map((part) => names[part as keyof typeof names] ?? part);
	return mac ? [...mods, key].join('') : [...mods, key].join('+');
}

/** Typing in a field, a text area or an editor: single keys like `/` belong to it. */
export function isTyping(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false;
	return (
		target.isContentEditable ||
		target instanceof HTMLInputElement ||
		target instanceof HTMLTextAreaElement ||
		target instanceof HTMLSelectElement
	);
}

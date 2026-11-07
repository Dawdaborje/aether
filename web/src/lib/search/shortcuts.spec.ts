import { describe, expect, it } from 'vitest';
import { formatShortcut, shortcutOf } from './shortcuts';

const press = (
	code: string,
	mods: Partial<Record<'alt' | 'shift' | 'ctrl' | 'meta', boolean>> = {}
) => ({
	code,
	altKey: !!mods.alt,
	shiftKey: !!mods.shift,
	ctrlKey: !!mods.ctrl,
	metaKey: !!mods.meta
});

describe('palette shortcuts', () => {
	it('reads key presses the way the server writes shortcuts', () => {
		expect(shortcutOf(press('KeyN', { alt: true }), false)).toBe('alt+n');
		expect(shortcutOf(press('KeyE', { ctrl: true, shift: true }), false)).toBe('mod+shift+e');
		expect(shortcutOf(press('KeyE', { meta: true, shift: true }), true)).toBe('mod+shift+e');
		expect(shortcutOf(press('Digit1', { alt: true, ctrl: true }), false)).toBe('mod+alt+1');
	});

	it('ignores plain keys, other keys and the other platform key', () => {
		expect(shortcutOf(press('KeyN'), false)).toBeNull();
		expect(shortcutOf(press('KeyN', { shift: true }), false)).toBeNull();
		expect(shortcutOf(press('Enter', { alt: true }), false)).toBeNull();
		expect(shortcutOf(press('KeyN', { ctrl: true }), true)).toBeNull();
		expect(shortcutOf(press('KeyN', { meta: true, alt: true }), false)).toBeNull();
	});

	it('shows shortcuts for the platform', () => {
		expect(formatShortcut('mod+shift+e', true)).toBe('⌘⇧E');
		expect(formatShortcut('alt+n', false)).toBe('Alt+N');
	});
});

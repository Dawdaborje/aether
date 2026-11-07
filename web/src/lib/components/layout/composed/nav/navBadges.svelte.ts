import { notificationsStore } from '$lib/notifications/notificationsStore.svelte';
import { callPlugin } from '$lib/pages/pageContext.svelte';
import type { NavBadge } from '$lib/theme/types';

/** How often counts are asked again while a badge is on screen. */
export const BADGE_INTERVAL_MS = 60_000;
/** A count is not asked again sooner than this, however often the page changes. */
const MIN_GAP_MS = 5_000;

/**
 * What a badge function answered, as the text to show: a positive number (`99+` above 99), the
 * length of a list, `count` of an object, or short text. Nothing (`null`) for zero, `false`,
 * empty answers and anything else.
 */
export function badgeText(answer: unknown): string | null {
	if (Array.isArray(answer)) return badgeText(answer.length);
	if (answer && typeof answer === 'object' && 'count' in answer) {
		return badgeText((answer as { count: unknown }).count);
	}
	if (typeof answer === 'number' && Number.isFinite(answer) && answer > 0) {
		return answer > 99 ? '99+' : String(Math.floor(answer));
	}
	if (typeof answer === 'string') {
		const text = answer.trim();
		return text && text.length <= 12 ? text : null;
	}
	return null;
}

/** The number an answer holds: a number, `{ count }` or the length of a list; else `null`. */
export function numberOf(answer: unknown): number | null {
	if (Array.isArray(answer)) return answer.length;
	if (answer && typeof answer === 'object' && 'count' in answer) {
		return numberOf((answer as { count: unknown }).count);
	}
	return typeof answer === 'number' && Number.isFinite(answer) ? answer : null;
}

/**
 * The counts of the badges on screen. Each badge registers its function while it is shown;
 * registered functions are asked when they first appear, every minute, when the window gets
 * focus again and after a navigation. Pushed too: when a plugin emits an event
 * (`events::emit`), its badges are asked again at once, and a new notification refreshes all. Each is called through the ordinary plugin call API, so
 * the function's own permission checks apply. A failing function shows no badge.
 */
class NavBadges {
	/** `plugin.function` → the text to show, or `null`. */
	values = $state<Record<string, string | null>>({});
	/** `plugin.function` → the number it answered (launcher tiles show it in full), or `null`. */
	numbers = $state<Record<string, number | null>>({});
	private users = new Map<string, number>();
	private asked = new Map<string, number>();
	private timer: ReturnType<typeof setInterval> | null = null;
	private unsubscribe: Array<() => void> = [];

	/** Show `fn`'s count while the caller is on screen; returns the function that stops it. */
	use(fn: string): () => void {
		this.users.set(fn, (this.users.get(fn) ?? 0) + 1);
		void this.ask(fn);
		this.start();
		return () => {
			const left = (this.users.get(fn) ?? 1) - 1;
			if (left > 0) this.users.set(fn, left);
			else this.users.delete(fn);
			if (this.users.size === 0) this.stop();
		};
	}

	/** Ask every shown function again (after a navigation or an action that changes counts). */
	refresh(force = false): void {
		for (const fn of this.users.keys()) void this.ask(fn, force);
	}

	/** Ask the shown functions of `plugin` again: it just said something changed. */
	refreshPlugin(plugin: string): void {
		for (const fn of this.users.keys()) {
			if (fn.startsWith(`${plugin}.`)) void this.ask(fn, true);
		}
	}

	private async ask(fn: string, force = false): Promise<void> {
		const now = Date.now();
		if (!force && now - (this.asked.get(fn) ?? 0) < MIN_GAP_MS) return;
		this.asked.set(fn, now);
		const dot = fn.indexOf('.');
		if (dot <= 0) return;
		const result = await callPlugin(fn.slice(0, dot), fn.slice(dot + 1));
		this.values[fn] = result.ok ? badgeText(result.data) : null;
		this.numbers[fn] = result.ok ? numberOf(result.data) : null;
	}

	private start() {
		if (this.timer || typeof window === 'undefined') return;
		this.timer = setInterval(() => this.refresh(true), BADGE_INTERVAL_MS);
		window.addEventListener('focus', this.onFocus);
		this.unsubscribe = [
			notificationsStore.onEvent((event) => this.refreshPlugin(event.plugin)),
			notificationsStore.onNotification(() => this.refresh(true))
		];
	}

	private stop() {
		if (this.timer) clearInterval(this.timer);
		this.timer = null;
		if (typeof window !== 'undefined') window.removeEventListener('focus', this.onFocus);
		for (const stop of this.unsubscribe) stop();
		this.unsubscribe = [];
	}

	private onFocus = () => this.refresh();
}

export const navBadges = new NavBadges();

/** The function a badge asks, or `null` for fixed text. */
export const badgeFunction = (badge: NavBadge | undefined): string | null =>
	badge && typeof badge === 'object' ? badge.function : null;

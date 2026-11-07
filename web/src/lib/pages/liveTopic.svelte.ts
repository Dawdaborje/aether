import { subscribeTopic } from '$lib/realtime/store.svelte';
import type { EventFrame, MessageFrame } from '$lib/realtime/client';
import { getPageState, type PageState } from './pageContext.svelte';

/**
 * The full topic name for a `topic="channel.{channel}"` attribute: the `{name}` and `{query.name}` parts are filled
 * from the page, and the page's plugin is put in front (`chat:channel.7`). `null` while a part is still empty (no
 * channel chosen yet), so nothing is joined.
 */
export function topicName(
	page: Pick<PageState, 'plugin' | 'vars'> | undefined,
	template: unknown
): string | null {
	if (typeof template !== 'string' || template.trim() === '' || !page) return null;
	const vars = page.vars;
	let missing = false;
	const filled = template.trim().replace(/\{((?:query\.)?[a-z0-9_]+)\}/g, (_, name: string) => {
		const value = vars[name];
		if (!value) missing = true;
		return value ?? '';
	});
	if (missing || filled.includes('{') || filled.includes('}')) return null;
	return filled.includes(':') ? filled : page.plugin ? `${page.plugin}:${filled}` : null;
}

export interface LiveTopic {
	/** Goes up (at most every `coalesceMs`) when something arrived on the topic; widgets read it to load again. */
	readonly n: number;
	/** Goes up when messages were missed beyond what the server keeps. */
	readonly resyncs: number;
	readonly joined: boolean;
	/** Why the server refused the topic, when it did. */
	readonly denied: string | null;
}

/**
 * Join the page's live topic while the widget is on screen. Call it while a component is being set up. A widget
 * that loads its data from a function adds `void live.n` to the effect that loads, and so loads again whenever
 * something is published on the topic. A widget that wants every message passes `onMessage`.
 */
export function useLiveTopic(
	template: () => unknown,
	options: {
		onMessage?: (message: MessageFrame) => void;
		onEvent?: (event: EventFrame) => void;
		onResync?: () => void;
		coalesceMs?: number;
	} = {}
): LiveTopic {
	const page = getPageState();
	const state = $state({ n: 0, resyncs: 0, joined: false, denied: null as string | null });
	const wait = options.coalesceMs ?? 150;

	$effect(() => {
		const topic = topicName(page, template());
		state.joined = false;
		state.denied = null;
		if (!topic) return;
		let timer: ReturnType<typeof setTimeout> | null = null;
		const bump = () => {
			if (timer !== null) return;
			timer = setTimeout(() => {
				timer = null;
				state.n += 1;
			}, wait);
		};
		const off = subscribeTopic(topic, {
			onJoined: () => {
				state.joined = true;
				state.denied = null;
			},
			onDenied: (reason) => {
				state.joined = false;
				state.denied = reason;
			},
			onMessage: (message) => {
				options.onMessage?.(message);
				bump();
			},
			onEvent: (event) => options.onEvent?.(event),
			onResync: () => {
				state.resyncs += 1;
				options.onResync?.();
				bump();
			}
		});
		return () => {
			off();
			if (timer !== null) clearTimeout(timer);
		};
	});

	return state;
}

export interface Presence {
	/** The people connected to the topic now (ids as the server names them). */
	readonly users: string[];
	/** Of those, who is not looking at it: on another page or app, or with the tab in the background. */
	readonly idle: string[];
	readonly joined: boolean;
}

/**
 * Join a topic that tracks presence and follow who is connected. Being on the page counts as being connected, so a
 * widget that only wants to show who is online (a rail of people) also makes the person count as online. With `keep`
 * the person stays on the topic, as idle, after they leave the page (and while the tab is in the background), so
 * going to another page or app shows them as idle rather than gone.
 */
export function usePresence(template: () => unknown, options: { keep?: boolean } = {}): Presence {
	const page = getPageState();
	const state = $state({ users: [] as string[], idle: [] as string[], joined: false });
	$effect(() => {
		const topic = topicName(page, template());
		state.users = [];
		state.idle = [];
		state.joined = false;
		if (!topic) return;
		return subscribeTopic(
			topic,
			{
				onJoined: (info) => {
					state.joined = true;
					state.users = [...info.presence];
					state.idle = [...info.idle];
				},
				onDenied: () => {
					state.joined = false;
					state.users = [];
					state.idle = [];
				},
				onPresence: (frame) => {
					if (frame.join && !state.users.includes(frame.join))
						state.users = [...state.users, frame.join];
					if (frame.leave) {
						state.users = state.users.filter((u) => u !== frame.leave);
						state.idle = state.idle.filter((u) => u !== frame.leave);
					}
					if (frame.idle && !state.idle.includes(frame.idle)) state.idle = [...state.idle, frame.idle];
					if (frame.active) state.idle = state.idle.filter((u) => u !== frame.active);
				}
			},
			{ keep: options.keep }
		);
	});
	return state;
}

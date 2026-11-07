import { describe, expect, it } from 'vitest';
import { RealtimeClient, type MessageFrame, type SocketLike } from './client';

class FakeSocket implements SocketLike {
	readyState = 0;
	sent: Record<string, unknown>[] = [];
	onopen: (() => void) | null = null;
	onmessage: ((event: { data: unknown }) => void) | null = null;
	onclose: (() => void) | null = null;
	onerror: (() => void) | null = null;
	send(data: string) {
		this.sent.push(JSON.parse(data));
	}
	close() {
		this.readyState = 3;
	}
	open() {
		this.readyState = 1;
		this.onopen?.();
	}
	say(frame: unknown) {
		this.onmessage?.({ data: JSON.stringify(frame) });
	}
	drop() {
		this.readyState = 3;
		this.onclose?.();
	}
}

function setup() {
	const sockets: FakeSocket[] = [];
	const timers: { fn: () => void; ms: number; live: boolean }[] = [];
	const client = new RealtimeClient({
		url: () => 'ws://test/api/ui/realtime',
		socket: () => {
			const s = new FakeSocket();
			sockets.push(s);
			return s;
		},
		setTimer: (fn, ms) => {
			const t = { fn, ms, live: true };
			timers.push(t);
			return t;
		},
		clearTimer: (h) => {
			(h as { live: boolean }).live = false;
		}
	});
	const fire = (ms: number) => {
		const t = timers.find((x) => x.live && x.ms === ms);
		if (!t) throw new Error(`no timer of ${ms}`);
		t.live = false;
		t.fn();
	};
	return { client, sockets, fire };
}

describe('realtime client', () => {
	it('opens on the first subscription and joins when the socket is up', () => {
		const { client, sockets } = setup();
		const got: MessageFrame[] = [];
		client.subscribe('chat:c.1', { onMessage: (m) => got.push(m) });
		expect(sockets).toHaveLength(1);
		sockets[0].open();
		expect(sockets[0].sent).toEqual([{ op: 'sub', topic: 'chat:c.1' }]);
		sockets[0].say({ t: 'sub', topic: 'chat:c.1', ok: true, seq: 5, presence: [] });
		sockets[0].say({ t: 'msg', topic: 'chat:c.1', seq: 6, event: 'message', payload: { id: 1 } });
		expect(got.map((m) => m.seq)).toEqual([6]);
	});

	it('shares one subscription between widgets and leaves when the last one goes', () => {
		const { client, sockets } = setup();
		const a = client.subscribe('chat:c', {});
		const b = client.subscribe('chat:c', {});
		sockets[0].open();
		expect(sockets[0].sent.filter((f) => f.op === 'sub')).toHaveLength(1);
		a();
		expect(sockets[0].sent.some((f) => f.op === 'unsub')).toBe(false);
		b();
		expect(sockets[0].sent.some((f) => f.op === 'unsub')).toBe(true);
	});

	it('rejoins from the last sequence after a drop, backing off, and skips a replay overlap', () => {
		const { client, sockets, fire } = setup();
		const got: number[] = [];
		client.subscribe('chat:c', { onMessage: (m) => got.push(m.seq) });
		sockets[0].open();
		sockets[0].say({ t: 'sub', topic: 'chat:c', ok: true, seq: 10, presence: [] });
		sockets[0].say({ t: 'msg', topic: 'chat:c', seq: 11, event: 'm', payload: null });
		sockets[0].drop();
		expect(client.connected).toBe(false);
		fire(1000);
		expect(sockets).toHaveLength(2);
		sockets[1].open();
		expect(sockets[1].sent).toEqual([{ op: 'sub', topic: 'chat:c', since: 11 }]);
		sockets[1].say({ t: 'sub', topic: 'chat:c', ok: true, seq: 13, presence: [] });
		sockets[1].say({ t: 'msg', topic: 'chat:c', seq: 11, event: 'm', payload: null });
		sockets[1].say({ t: 'msg', topic: 'chat:c', seq: 12, event: 'm', payload: null });
		sockets[1].say({ t: 'msg', topic: 'chat:c', seq: 13, event: 'm', payload: null });
		expect(got).toEqual([11, 12, 13]);
	});

	it('doubles the wait while the server is unreachable', () => {
		const { client, sockets, fire } = setup();
		client.subscribe('chat:c', {});
		sockets[0].drop();
		fire(1000);
		sockets[1].drop();
		fire(2000);
		sockets[2].drop();
		fire(4000);
		expect(sockets).toHaveLength(4);
	});

	it('tells the handler about a resync, a refusal and a removal', () => {
		const { client, sockets } = setup();
		const log: string[] = [];
		client.subscribe('chat:c', {
			onResync: () => log.push('resync'),
			onDenied: (r) => log.push(`denied:${r}`)
		});
		sockets[0].open();
		sockets[0].say({ t: 'resync', topic: 'chat:c' });
		sockets[0].say({ t: 'sub', topic: 'chat:c', ok: false, reason: 'not allowed' });
		sockets[0].say({ t: 'dropped', topic: 'chat:c' });
		expect(log).toEqual(['resync', 'denied:not allowed', 'denied:removed']);
	});

	it('sends client events only on a joined topic', () => {
		const { client, sockets } = setup();
		client.subscribe('chat:c', {});
		expect(client.sendEvent('chat:c', 'typing')).toBe(false);
		sockets[0].open();
		expect(client.sendEvent('chat:c', 'typing')).toBe(false);
		sockets[0].say({ t: 'sub', topic: 'chat:c', ok: true, seq: 0, presence: [] });
		expect(client.sendEvent('chat:c', 'typing', { a: 1 })).toBe(true);
		expect(sockets[0].sent.at(-1)).toEqual({
			op: 'event',
			topic: 'chat:c',
			kind: 'typing',
			payload: { a: 1 }
		});
	});

	it('gives a second widget the presence the first one was told', () => {
		const { client, sockets } = setup();
		client.subscribe('chat:c', {});
		sockets[0].open();
		sockets[0].say({ t: 'sub', topic: 'chat:c', ok: true, seq: 0, presence: ['ann'] });
		sockets[0].say({ t: 'presence', topic: 'chat:c', join: 'bob' });
		sockets[0].say({ t: 'presence', topic: 'chat:c', leave: 'ann' });
		let seen: string[] = [];
		client.subscribe('chat:c', { onJoined: (i) => (seen = i.presence) });
		expect(seen).toEqual(['bob']);
	});

	it('closes after the last topic has been gone for a while', () => {
		const { client, sockets, fire } = setup();
		const off = client.subscribe('chat:c', {});
		sockets[0].open();
		off();
		expect(sockets[0].readyState).toBe(1);
		fire(5000);
		expect(sockets[0].readyState).toBe(3);
	});

	describe('a kept topic (presence that outlives the page)', () => {
		const joined = (sockets: FakeSocket[]) => {
			sockets[0].open();
			sockets[0].say({ t: 'sub', topic: 'chat:online', ok: true, seq: 0, presence: ['ann'], idle: [] });
		};

		it('stays on the topic as idle when the last widget leaves, and keeps the socket open', () => {
			const { client, sockets, fire } = setup();
			const off = client.subscribe('chat:online', {}, { keep: true });
			joined(sockets);
			sockets[0].sent.length = 0;
			off();
			expect(sockets[0].sent).toEqual([{ op: 'idle', topic: 'chat:online', idle: true }]);
			expect(() => fire(5_000)).toThrow(); // no timer to close the socket was started
			expect(sockets[0].readyState).toBe(1);
		});

		it('is active again when a widget comes back, and says so once', () => {
			const { client, sockets } = setup();
			client.subscribe('chat:online', {}, { keep: true })();
			joined(sockets);
			sockets[0].sent.length = 0;
			let seen: string[] = [];
			client.subscribe('chat:online', { onJoined: (i) => (seen = i.presence) }, { keep: true });
			expect(seen).toEqual(['ann']);
			expect(sockets[0].sent).toEqual([{ op: 'idle', topic: 'chat:online', idle: false }]);
		});

		it('is idle while the page is in the background, even with a widget on it', () => {
			const { client, sockets } = setup();
			client.subscribe('chat:online', {}, { keep: true });
			joined(sockets);
			sockets[0].sent.length = 0;
			client.setVisible(false);
			client.setVisible(false);
			client.setVisible(true);
			expect(sockets[0].sent).toEqual([
				{ op: 'idle', topic: 'chat:online', idle: true },
				{ op: 'idle', topic: 'chat:online', idle: false }
			]);
		});

		it('says it is idle right after joining when nobody is looking', () => {
			const { client, sockets } = setup();
			client.subscribe('chat:online', {}, { keep: true })();
			sockets[0].open();
			sockets[0].sent.length = 0;
			sockets[0].say({ t: 'sub', topic: 'chat:online', ok: true, seq: 0, presence: [], idle: [] });
			expect(sockets[0].sent).toEqual([{ op: 'idle', topic: 'chat:online', idle: true }]);
		});

		it('joins again after the connection drops and reports its state again', () => {
			const { client, sockets, fire } = setup();
			client.subscribe('chat:online', {}, { keep: true })();
			joined(sockets);
			sockets[0].drop();
			fire(1000);
			sockets[1].open();
			expect(sockets[1].sent[0]).toMatchObject({ op: 'sub', topic: 'chat:online' });
			sockets[1].say({ t: 'sub', topic: 'chat:online', ok: true, seq: 0, presence: [], idle: [] });
			expect(sockets[1].sent).toContainEqual({ op: 'idle', topic: 'chat:online', idle: true });
		});

		it('tells widgets who is idle, from the join answer and from later frames', () => {
			const { client, sockets } = setup();
			let idle: string[] = [];
			const frames: string[] = [];
			client.subscribe(
				'chat:online',
				{ onJoined: (i) => (idle = i.idle), onPresence: (f) => frames.push(f.idle ? `idle:${f.idle}` : `active:${f.active}`) },
				{ keep: true }
			);
			sockets[0].open();
			sockets[0].say({ t: 'sub', topic: 'chat:online', ok: true, seq: 0, presence: ['ann', 'bob'], idle: ['bob'] });
			expect(idle).toEqual(['bob']);
			sockets[0].say({ t: 'presence', topic: 'chat:online', idle: 'ann' });
			sockets[0].say({ t: 'presence', topic: 'chat:online', active: 'bob' });
			expect(frames).toEqual(['idle:ann', 'active:bob']);
		});

		it('a held topic is the session being there: idle until a widget looks, active while one does', () => {
			const { client, sockets } = setup();
			const release = client.hold('kernel:online');
			sockets[0].open();
			expect(sockets[0].sent).toEqual([{ op: 'sub', topic: 'kernel:online' }]);
			sockets[0].sent.length = 0;
			sockets[0].say({ t: 'sub', topic: 'kernel:online', ok: true, seq: 0, presence: [], idle: [] });
			expect(sockets[0].sent).toEqual([{ op: 'idle', topic: 'kernel:online', idle: true }]);
			sockets[0].sent.length = 0;
			const off = client.subscribe('kernel:online', {}, { keep: true });
			expect(sockets[0].sent).toEqual([{ op: 'idle', topic: 'kernel:online', idle: false }]);
			sockets[0].sent.length = 0;
			off();
			expect(sockets[0].sent).toEqual([{ op: 'idle', topic: 'kernel:online', idle: true }]);
			sockets[0].sent.length = 0;
			release();
			expect(sockets[0].sent).toEqual([{ op: 'unsub', topic: 'kernel:online' }]);
		});

		it('an ordinary topic is idle while the tab is in the background', () => {
			const { client, sockets } = setup();
			client.subscribe('chat:channel.1', {});
			sockets[0].open();
			sockets[0].say({ t: 'sub', topic: 'chat:channel.1', ok: true, seq: 0, presence: [], idle: [] });
			sockets[0].sent.length = 0;
			client.setVisible(false);
			client.setVisible(true);
			expect(sockets[0].sent).toEqual([
				{ op: 'idle', topic: 'chat:channel.1', idle: true },
				{ op: 'idle', topic: 'chat:channel.1', idle: false }
			]);
		});

		it('a topic that is not kept still leaves and lets the socket close', () => {
			const { client, sockets, fire } = setup();
			const off = client.subscribe('chat:c', {});
			sockets[0].open();
			sockets[0].sent.length = 0;
			off();
			expect(sockets[0].sent).toEqual([{ op: 'unsub', topic: 'chat:c' }]);
			fire(5_000);
			expect(sockets[0].readyState).toBe(3);
		});
	});
});

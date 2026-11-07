import { describe, expect, it } from 'vitest';
import { CallClient, signalingUrl, type PeerLike, type SocketLike } from './client';

class FakeSocket implements SocketLike {
	sent: Record<string, unknown>[] = [];
	closed = false;
	onopen = null;
	onmessage: SocketLike['onmessage'] = null;
	onclose: SocketLike['onclose'] = null;
	onerror = null;
	send(data: string) {
		this.sent.push(JSON.parse(data));
	}
	close() {
		this.closed = true;
	}
	receive(frame: Record<string, unknown>) {
		this.onmessage?.({ data: JSON.stringify(frame) });
	}
}

class FakePeer implements PeerLike {
	localDescription: { sdp: string } | null = null;
	ontrack: PeerLike['ontrack'] = null;
	onicecandidate: PeerLike['onicecandidate'] = null;
	added: MediaStreamTrack[] = [];
	remote: RTCSessionDescriptionInit | null = null;
	closed = false;
	constructor(readonly config: RTCConfiguration) {}
	transceivers: { track: MediaStreamTrack; init: RTCRtpTransceiverInit }[] = [];
	addTransceiver(track: MediaStreamTrack, init: RTCRtpTransceiverInit) {
		this.transceivers.push({ track, init });
		this.addTrack(track);
		return {} as RTCRtpTransceiver;
	}
	senders: { track: MediaStreamTrack | null; replaceTrack(t: MediaStreamTrack | null): Promise<void> }[] = [];
	addTrack(track: MediaStreamTrack) {
		this.added.push(track);
		const sender = {
			track: track as MediaStreamTrack | null,
			async replaceTrack(next: MediaStreamTrack | null) {
				sender.track = next;
			}
		};
		this.senders.push(sender);
		return sender as unknown as RTCRtpSender;
	}
	getSenders() {
		return this.senders as unknown as RTCRtpSender[];
	}
	getTransceivers() {
		return [] as RTCRtpTransceiver[];
	}
	async createOffer() {
		return { type: 'offer' as const, sdp: 'local-offer' };
	}
	async createAnswer() {
		return { type: 'answer' as const, sdp: 'local-answer' };
	}
	async setLocalDescription(d?: RTCSessionDescriptionInit) {
		this.localDescription = { sdp: d?.sdp ?? '' };
	}
	async setRemoteDescription(d: RTCSessionDescriptionInit) {
		this.remote = d;
	}
	async addIceCandidate() {}
	close() {
		this.closed = true;
	}
}

const track = (kind: string): MediaStreamTrack => ({ kind, enabled: true, stop() {} }) as unknown as MediaStreamTrack;
const media = (...tracks: MediaStreamTrack[]): MediaStream =>
	({
		id: 'local',
		getTracks: () => tracks,
		getAudioTracks: () => tracks.filter((t) => t.kind === 'audio'),
		getVideoTracks: () => tracks.filter((t) => t.kind === 'video'),
		addTrack: (t: MediaStreamTrack) => void tracks.push(t),
		removeTrack: (t: MediaStreamTrack) => void tracks.splice(tracks.indexOf(t), 1)
	}) as unknown as MediaStream;

const ann = { sid: 's1', user: 'users:ann', name: 'Ann', role: 'host', audio: true, video: true, screen: false, since: 1 };
const bob = { sid: 's2', user: 'users:bob', name: 'Bob', role: 'member', audio: true, video: true, screen: false, since: 2 };

function setup(stream: MediaStream | null = media(track('audio'), track('video')), display?: () => Promise<MediaStream>) {
	const socket = new FakeSocket();
	const peers: FakePeer[] = [];
	let changes = 0;
	const client = new CallClient({
		url: 'ws://x/api/ui/sfu/meet:m.1',
		stream,
		display,
		socket: () => socket,
		peer: (config) => {
			const peer = new FakePeer(config);
			peers.push(peer);
			return peer;
		},
		onChange: () => changes++
	});
	const flush = () => new Promise((r) => setTimeout(r, 0));
	return { socket, peers, client, flush, changes: () => changes };
}

describe('joining', () => {
	it('starts joining, then publishes what the person sends once the server says joined', async () => {
		const { socket, peers, client, flush } = setup();
		client.start();
		expect(client.state).toBe('joining');
		socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [{ urls: ['turn:t'] }], participants: [ann] });
		await flush();
		expect(client.state).toBe('live');
		expect(client.role).toBe('host');
		expect(client.me?.name).toBe('Ann');
		expect(peers).toHaveLength(2);
		expect(peers[0].config.iceServers).toEqual([{ urls: ['turn:t'] }]);
		expect(peers[0].added.map((t) => t.kind)).toEqual(['audio', 'video']);
		expect(socket.sent).toContainEqual({ op: 'offer', sdp: 'local-offer' });
	});

	it('sends the camera as three layers and the microphone as one', async () => {
		const { socket, peers, client, flush } = setup();
		client.start();
		socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [], participants: [ann] });
		await flush();
		expect(peers[0].transceivers).toHaveLength(1);
		expect(peers[0].transceivers[0].track.kind).toBe('video');
		expect(peers[0].transceivers[0].init.sendEncodings?.map((e) => e.rid)).toEqual(['h', 'm', 'l']);
		expect(peers[0].added.map((t) => t.kind).sort()).toEqual(['audio', 'video']);
	});

	it('sends one stream when simulcast is off', async () => {
		const s = setup();
		(s.client as unknown as { options: { simulcast?: boolean } }).options.simulcast = false;
		s.client.start();
		s.socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [], participants: [ann] });
		await s.flush();
		expect(s.peers[0].transceivers).toHaveLength(0);
	});

	it('only listens when there is nothing to send', async () => {
		const { socket, peers, client, flush } = setup(null);
		client.start();
		socket.receive({ t: 'joined', you: 'users:ann', role: 'member', ice: [], participants: [ann] });
		await flush();
		expect(peers[0].added).toHaveLength(0);
		expect(socket.sent.some((f) => f.op === 'offer')).toBe(false);
	});

	it('fails with the reason when the server refuses', async () => {
		const { socket, client, flush } = setup();
		client.start();
		socket.receive({ t: 'error', message: 'this meeting is full' });
		await flush();
		expect(client.state).toBe('failed');
		expect(client.reason).toBe('this meeting is full');
		expect(socket.closed).toBe(true);
	});

	it('fails when the socket closes before joining', () => {
		const { socket, client } = setup();
		client.start();
		socket.onclose?.({});
		expect(client.state).toBe('failed');
		expect(client.reason).toMatch(/could not be reached/);
	});
});

describe('in a meeting', () => {
	async function live() {
		const s = setup();
		s.client.start();
		s.socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [], participants: [ann] });
		await s.flush();
		return s;
	}

	it('answers the server offer on the receiving connection', async () => {
		const { socket, peers, flush } = await live();
		socket.receive({ t: 'offer', sdp: 'server-offer' });
		await flush();
		expect(peers[1].remote).toEqual({ type: 'offer', sdp: 'server-offer' });
		expect(socket.sent).toContainEqual({ op: 'answer', sdp: 'local-answer' });
	});

	it('applies the answer to the publisher offer', async () => {
		const { socket, peers, flush } = await live();
		socket.receive({ t: 'answer', sdp: 'server-answer' });
		await flush();
		expect(peers[0].remote).toEqual({ type: 'answer', sdp: 'server-answer' });
	});

	it('keeps the roster as people join, change and leave', async () => {
		const { socket, client, flush } = await live();
		socket.receive({ t: 'participant', change: { join: bob } });
		socket.receive({ t: 'participant', change: { update: { ...bob, audio: false } } });
		await flush();
		expect(client.roster.map((p) => p.user).sort()).toEqual(['users:ann', 'users:bob']);
		expect(client.roster.find((p) => p.user === 'users:bob')?.audio).toBe(false);
		socket.receive({ t: 'participant', change: { leave: { user: 'users:bob' } } });
		await flush();
		expect(client.roster.map((p) => p.user)).toEqual(['users:ann']);
	});

	it('collects remote media by stream id and drops it when the person leaves', async () => {
		const { socket, peers, client, flush } = await live();
		socket.receive({ t: 'participant', change: { join: bob } });
		await flush();
		const stream = { id: 's2' } as MediaStream;
		peers[1].ontrack?.({ streams: [stream], track: track('video') });
		expect(client.streams.get('s2')).toBe(stream);
		socket.receive({ t: 'participant', change: { leave: { user: 'users:bob' } } });
		await flush();
		expect(client.streams.has('s2')).toBe(false);
	});

	it('tells the server a change of wish once, not on every repeat', async () => {
		const { socket, client } = await live();
		client.want('users:bob', 'low');
		client.want('users:bob', 'low');
		client.want('users:bob', 'high');
		expect(socket.sent.filter((f) => f.op === 'want').map((f) => f.quality)).toEqual(['low', 'high']);
	});

	it('mutes its own tracks and tells the others', async () => {
		const { socket, client } = await live();
		client.setMuted('audio', true);
		expect(client.audioOn).toBe(false);
		expect(client.stream?.getAudioTracks()[0].enabled).toBe(false);
		expect(client.stream?.getVideoTracks()[0].enabled).toBe(true);
		expect(socket.sent).toContainEqual({ op: 'mute', kind: 'audio', muted: true });
	});

	it('goes quiet when a host mutes it, and can be switched on again by the person', async () => {
		const { socket, client, flush } = await live();
		socket.receive({ t: 'muted', kind: 'audio' });
		await flush();
		expect(client.audioOn).toBe(false);
		expect(client.stream?.getAudioTracks()[0].enabled).toBe(false);
		expect(client.stream?.getVideoTracks()[0].enabled).toBe(true);
		client.setMuted('audio', false);
		expect(client.audioOn).toBe(true);
	});

	it('shares a screen: tells the server first, then offers the new video line; stopping tells it again', async () => {
		const screenTrack = { ...track('video'), stopped: false, stop() { this.stopped = true; } };
		const screen = media(screenTrack as unknown as MediaStreamTrack);
		const s = setup(undefined, async () => screen);
		s.client.start();
		s.socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [], participants: [ann] });
		await s.flush();
		s.socket.sent.length = 0;
		expect(await s.client.shareScreen()).toBe(true);
		expect(s.socket.sent.map((f) => f.op)).toEqual(['screen', 'offer']);
		expect(s.socket.sent[0]).toEqual({ op: 'screen', sharing: true });
		expect(s.peers[0].added).toContain(screenTrack);
		expect(s.client.sharing).toBe(screen);
		expect(await s.client.shareScreen()).toBe(false); // one at a time
		s.client.stopScreen();
		expect(screenTrack.stopped).toBe(true);
		expect(s.client.sharing).toBeNull();
		expect(s.socket.sent.at(-1)).toEqual({ op: 'screen', sharing: false });
	});

	it('does not share when the person closes the picker', async () => {
		const s = setup(undefined, async () => { throw new Error('closed'); });
		s.client.start();
		s.socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [], participants: [ann] });
		await s.flush();
		expect(await s.client.shareScreen()).toBe(false);
		expect(s.socket.sent.some((f) => f.op === 'screen')).toBe(false);
	});

	it("shows another person's shared screen as a stream of its own and drops it when they stop", async () => {
		const { socket, peers, client, flush } = await live();
		socket.receive({ t: 'participant', change: { join: bob } });
		await flush();
		const screenStream = { id: 's2-screen' } as unknown as MediaStream;
		peers[1].ontrack?.({ streams: [screenStream], track: {} as MediaStreamTrack });
		await flush();
		expect(client.screenOf('s2')).toBe(screenStream);
		socket.receive({ t: 'participant', change: { update: { ...bob, screen: true } } });
		await flush();
		expect(client.screenOf('s2')).toBe(screenStream);
		socket.receive({ t: 'participant', change: { update: { ...bob, screen: false } } });
		await flush();
		expect(client.screenOf('s2')).toBeUndefined();
	});

	it('swaps a microphone or camera without a new offer, keeping the muted state, and lets the old one go', async () => {
		const oldCamera = { ...track('video'), stopped: false, stop() { this.stopped = true; } };
		const mic = track('audio');
		const s = setup(media(mic, oldCamera as unknown as MediaStreamTrack));
		s.client.start();
		s.socket.receive({ t: 'joined', you: 'users:ann', role: 'host', ice: [], participants: [ann] });
		await s.flush();
		const offers = s.socket.sent.filter((f) => f.op === 'offer').length;
		s.client.setMuted('video', true);
		const newCamera = track('video');
		await s.client.switchDevice('video', newCamera);
		expect(s.peers[0].senders.find((x) => x.track === newCamera)).toBeDefined();
		expect(oldCamera.stopped).toBe(true);
		expect(s.client.stream?.getVideoTracks()).toEqual([newCamera]);
		expect(newCamera.enabled).toBe(false); // still off, as the person left it
		expect(s.client.stream?.getAudioTracks()).toEqual([mic]);
		expect(s.socket.sent.filter((f) => f.op === 'offer')).toHaveLength(offers);
	});

	it('ends when removed, with the reason, and lets go of everything', async () => {
		const { socket, peers, client, flush } = await live();
		socket.receive({ t: 'removed', reason: 'the meeting ended' });
		await flush();
		expect(client.state).toBe('ended');
		expect(client.reason).toBe('the meeting ended');
		expect(peers.every((p) => p.closed)).toBe(true);
		expect(socket.closed).toBe(true);
	});

	it('leaves on request', async () => {
		const { socket, client } = await live();
		client.leave();
		expect(socket.sent).toContainEqual({ op: 'leave' });
		expect(client.state).toBe('ended');
	});

	it('says it was disconnected when the socket drops mid-call', async () => {
		const { socket, client } = await live();
		socket.onclose?.({});
		expect(client.state).toBe('ended');
		expect(client.reason).toBe('You were disconnected.');
	});

	it('handles frames in order even when an offer takes time', async () => {
		const { socket, peers, flush } = await live();
		socket.receive({ t: 'offer', sdp: 'one' });
		socket.receive({ t: 'offer', sdp: 'two' });
		await flush();
		await flush();
		expect(peers[1].remote?.sdp).toBe('two');
		expect(socket.sent.filter((f) => f.op === 'answer')).toHaveLength(2);
	});
});

describe('signalingUrl', () => {
	it('builds a ws address for the room, with the organization when there is one', () => {
		expect(signalingUrl('meet:m.1', null, { protocol: 'http:', host: 'a.test' })).toBe('ws://a.test/api/ui/sfu/meet%3Am.1');
		expect(signalingUrl('meet:m.1', 'acme', { protocol: 'https:', host: 'a.test' })).toBe('wss://a.test/api/ui/sfu/meet%3Am.1?org=acme');
	});
});

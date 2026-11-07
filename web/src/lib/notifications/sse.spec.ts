import { describe, expect, it } from 'vitest';
import { readEventStream, type SseMessage } from './sse';

/** A body that delivers `chunks` one at a time, as a network would. */
function body(...chunks: string[]): ReadableStream<Uint8Array> {
	const encoder = new TextEncoder();
	return new ReadableStream({
		start(controller) {
			for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
			controller.close();
		}
	});
}

async function collect(...chunks: string[]): Promise<SseMessage[]> {
	const messages: SseMessage[] = [];
	await readEventStream(body(...chunks), (message) => messages.push(message));
	return messages;
}

describe('readEventStream', () => {
	it('reads events with their id, name and data', async () => {
		const messages = await collect('id: 01\nevent: notification\ndata: {"a":1}\n\n');
		expect(messages).toEqual([{ id: '01', event: 'notification', data: '{"a":1}' }]);
	});

	it('ignores keep-alive comments', async () => {
		const messages = await collect(': keep-alive\n\n', 'event: resync\ndata: \n\n');
		expect(messages.map((m) => m.event)).toEqual(['resync']);
	});

	it('puts together an event split across chunks, even mid-line', async () => {
		const messages = await collect('id: 0', '2\neven', 't: notification\nda', 'ta: hello\n', '\n');
		expect(messages).toEqual([{ id: '02', event: 'notification', data: 'hello' }]);
	});

	it('joins multi-line data and handles CRLF', async () => {
		const messages = await collect('data: one\r\ndata: two\r\n\r\n');
		expect(messages).toEqual([{ id: null, event: 'message', data: 'one\ntwo' }]);
	});

	it('delivers several events from one chunk, in order', async () => {
		const messages = await collect('id: 1\ndata: a\n\nid: 2\ndata: b\n\n');
		expect(messages.map((m) => m.id)).toEqual(['1', '2']);
	});

	it('does not deliver an event that never finished', async () => {
		expect(await collect('id: 1\ndata: partial')).toEqual([]);
	});
});

/** One server-sent event, as the server wrote it. */
export interface SseMessage {
	event: string;
	data: string;
	id: string | null;
}

/**
 * Read a server-sent-events body.
 *
 * `EventSource` cannot send headers, and the web app needs `X-Org-Slug` (header
 * tenancy) as well as `Last-Event-ID`, so the stream is read from a plain `fetch`.
 * Calls `onMessage` for each event until the body ends or `signal` aborts.
 */
export async function readEventStream(
	body: ReadableStream<Uint8Array>,
	onMessage: (message: SseMessage) => void
): Promise<void> {
	const reader = body.getReader();
	const decoder = new TextDecoder();
	let buffer = '';
	let event = 'message';
	let data: string[] = [];
	let id: string | null = null;

	const dispatch = () => {
		if (data.length > 0 || event !== 'message') onMessage({ event, data: data.join('\n'), id });
		event = 'message';
		data = [];
		id = null;
	};

	for (;;) {
		const { value, done } = await reader.read();
		if (done) return;
		buffer += decoder.decode(value, { stream: true });
		let newline: number;
		while ((newline = buffer.search(/\r\n|\n|\r/)) >= 0) {
			const line = buffer.slice(0, newline);
			buffer = buffer.slice(newline).replace(/^(\r\n|\n|\r)/, '');
			if (line === '') dispatch();
			else if (line.startsWith(':')) continue; // a keep-alive comment
			else {
				const colon = line.indexOf(':');
				const field = colon < 0 ? line : line.slice(0, colon);
				const content = colon < 0 ? '' : line.slice(colon + 1).replace(/^ /, '');
				if (field === 'event') event = content;
				else if (field === 'data') data.push(content);
				else if (field === 'id') id = content;
			}
		}
	}
}

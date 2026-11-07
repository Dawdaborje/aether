/**
 * A reader for `.eml` files (RFC 822 messages): headers, the text or HTML body, and the list of attachments.
 * Attachments are listed, never opened. Anything it cannot decode is left as it is.
 */
export interface EmailPart {
	name: string;
	type: string;
	size: number;
}
export interface Email {
	headers: { from: string; to: string; cc: string; date: string; subject: string };
	text: string;
	html: string;
	attachments: EmailPart[];
}

interface Raw {
	headers: Map<string, string>;
	body: string;
}

function splitMessage(source: string): Raw {
	const text = source.replace(/\r\n?/g, '\n');
	const cut = text.indexOf('\n\n');
	const head = cut === -1 ? text : text.slice(0, cut);
	const body = cut === -1 ? '' : text.slice(cut + 2);
	const headers = new Map<string, string>();
	let last = '';
	for (const line of head.split('\n')) {
		if (/^[ \t]/.test(line) && last) headers.set(last, `${headers.get(last)} ${line.trim()}`);
		else {
			const at = line.indexOf(':');
			if (at > 0) {
				last = line.slice(0, at).toLowerCase();
				if (!headers.has(last)) headers.set(last, line.slice(at + 1).trim());
			}
		}
	}
	return { headers, body };
}

function bytesOf(binary: string): Uint8Array {
	const out = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i) & 0xff;
	return out;
}

function decodeBytes(bytes: Uint8Array, charset: string): string {
	try {
		return new TextDecoder(charset || 'utf-8').decode(bytes);
	} catch {
		return new TextDecoder().decode(bytes);
	}
}

function quotedPrintable(text: string): string {
	const joined = text.replace(/=\n/g, '');
	const out: number[] = [];
	for (let i = 0; i < joined.length; i++) {
		const hex = joined.slice(i + 1, i + 3);
		if (joined[i] === '=' && /^[0-9A-Fa-f]{2}$/.test(hex)) {
			out.push(parseInt(hex, 16));
			i += 2;
		} else out.push(joined.charCodeAt(i) & 0xff);
	}
	return String.fromCharCode(...out);
}

function decodeBody(body: string, encoding: string, charset: string): string {
	const enc = encoding.toLowerCase();
	if (enc === 'base64') {
		try {
			return decodeBytes(bytesOf(atob(body.replace(/\s+/g, ''))), charset);
		} catch {
			return body;
		}
	}
	if (enc === 'quoted-printable') return decodeBytes(bytesOf(quotedPrintable(body)), charset);
	return body;
}

/** `=?utf-8?B?...?=` and `=?utf-8?Q?...?=` words in a header. */
export function decodeWords(value: string): string {
	return value
		.replace(/\?=\s+=\?/g, '?==?')
		.replace(
			/=\?([^?]+)\?([bBqQ])\?([^?]*)\?=/g,
			(_, charset: string, kind: string, data: string) => {
				try {
					if (kind.toLowerCase() === 'b') return decodeBytes(bytesOf(atob(data)), charset);
					return decodeBytes(bytesOf(quotedPrintable(data.replace(/_/g, ' '))), charset);
				} catch {
					return data;
				}
			}
		);
}

function param(header: string, name: string): string {
	const m = new RegExp(`${name}\\*?=\\s*(?:"([^"]*)"|([^;\\s]+))`, 'i').exec(header);
	return m ? (m[1] ?? m[2] ?? '') : '';
}

function walk(raw: Raw, out: Email, depth: number): void {
	const type = (raw.headers.get('content-type') ?? 'text/plain').toLowerCase();
	const main = type.split(';')[0].trim();
	const disposition = (raw.headers.get('content-disposition') ?? '').toLowerCase();
	if (main.startsWith('multipart/') && depth < 6) {
		const boundary = param(raw.headers.get('content-type') ?? '', 'boundary');
		if (!boundary) return;
		for (const piece of raw.body.split(`--${boundary}`).slice(1)) {
			if (piece.startsWith('--')) break;
			walk(splitMessage(piece.replace(/^\n/, '')), out, depth + 1);
		}
		return;
	}
	const filename = decodeWords(
		param(raw.headers.get('content-disposition') ?? '', 'filename') ||
			param(raw.headers.get('content-type') ?? '', 'name')
	);
	const encoding = raw.headers.get('content-transfer-encoding') ?? '7bit';
	if (disposition.startsWith('attachment') || (filename && !main.startsWith('text/'))) {
		out.attachments.push({ name: filename || 'attachment', type: main, size: raw.body.length });
		return;
	}
	const charset = param(raw.headers.get('content-type') ?? '', 'charset') || 'utf-8';
	if (main === 'text/plain' && !out.text) out.text = decodeBody(raw.body, encoding, charset).trim();
	else if (main === 'text/html' && !out.html) out.html = decodeBody(raw.body, encoding, charset);
}

export function parseEmail(source: string): Email {
	const root = splitMessage(source);
	const h = (name: string) => decodeWords(root.headers.get(name) ?? '');
	const email: Email = {
		headers: { from: h('from'), to: h('to'), cc: h('cc'), date: h('date'), subject: h('subject') },
		text: '',
		html: '',
		attachments: []
	};
	walk(root, email, 0);
	return email;
}

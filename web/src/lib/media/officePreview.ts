/**
 * A text preview of Office files (docx, xlsx, pptx) that needs no library: these are zip archives of XML,
 * and the browser can inflate the entries itself (`DecompressionStream`). Only the words and cells are
 * shown, not the layout. Anything it cannot read throws, and the viewer falls back to a download.
 */

interface Entry {
	method: number;
	compressedSize: number;
	offset: number;
}

const MAX_ENTRY_BYTES = 8 * 1024 * 1024;

async function readEntries(bytes: Uint8Array): Promise<Map<string, Entry>> {
	const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
	let eocd = -1;
	for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 70_000); i--) {
		if (view.getUint32(i, true) === 0x06054b50) {
			eocd = i;
			break;
		}
	}
	if (eocd < 0) throw new Error('not a zip archive');
	const count = view.getUint16(eocd + 10, true);
	let at = view.getUint32(eocd + 16, true);
	const decoder = new TextDecoder();
	const entries = new Map<string, Entry>();
	for (let n = 0; n < count; n++) {
		if (view.getUint32(at, true) !== 0x02014b50) throw new Error('damaged zip directory');
		const method = view.getUint16(at + 10, true);
		const compressedSize = view.getUint32(at + 20, true);
		const nameLen = view.getUint16(at + 28, true);
		const extraLen = view.getUint16(at + 30, true);
		const commentLen = view.getUint16(at + 32, true);
		const offset = view.getUint32(at + 42, true);
		const name = decoder.decode(bytes.subarray(at + 46, at + 46 + nameLen));
		entries.set(name, { method, compressedSize, offset });
		at += 46 + nameLen + extraLen + commentLen;
	}
	return entries;
}

async function readText(bytes: Uint8Array, entry: Entry): Promise<string> {
	const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
	const nameLen = view.getUint16(entry.offset + 26, true);
	const extraLen = view.getUint16(entry.offset + 28, true);
	const start = entry.offset + 30 + nameLen + extraLen;
	const raw = bytes.subarray(start, start + entry.compressedSize);
	if (entry.method === 0) return new TextDecoder().decode(raw);
	if (entry.method !== 8) throw new Error('unsupported compression');
	if (typeof DecompressionStream === 'undefined') throw new Error('this browser cannot unpack the file');
	const stream = new Blob([raw as BlobPart]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
	const reader = stream.getReader();
	const chunks: Uint8Array[] = [];
	let total = 0;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		total += value.length;
		if (total > MAX_ENTRY_BYTES) throw new Error('the file is too large to preview');
		chunks.push(value);
	}
	const out = new Uint8Array(total);
	let pos = 0;
	for (const c of chunks) {
		out.set(c, pos);
		pos += c.length;
	}
	return new TextDecoder().decode(out);
}

function xml(text: string): Document {
	return new DOMParser().parseFromString(text, 'application/xml');
}

function columnIndex(ref: string): number {
	let n = 0;
	for (const ch of ref.replace(/[0-9]/g, '')) n = n * 26 + (ch.charCodeAt(0) - 64);
	return Math.max(0, n - 1);
}

export type OfficePreview =
	| { kind: 'text'; paragraphs: string[] }
	| { kind: 'table'; rows: string[][]; sheet: string };

export async function previewOffice(buffer: ArrayBuffer, extension: string): Promise<OfficePreview> {
	const bytes = new Uint8Array(buffer);
	const entries = await readEntries(bytes);
	const get = async (name: string) => {
		const entry = entries.get(name);
		if (!entry) throw new Error(`${name} is missing`);
		return readText(bytes, entry);
	};
	if (extension === 'docx') {
		const doc = xml(await get('word/document.xml'));
		const paragraphs = Array.from(doc.getElementsByTagNameNS('*', 'p')).map((p) =>
			Array.from(p.getElementsByTagNameNS('*', 't'))
				.map((t) => t.textContent ?? '')
				.join('')
		);
		return { kind: 'text', paragraphs: paragraphs.slice(0, 2000) };
	}
	if (extension === 'pptx') {
		const names = Array.from(entries.keys())
			.filter((n) => /^ppt\/slides\/slide\d+\.xml$/.test(n))
			.sort((a, b) => parseInt(a.replace(/\D/g, '')) - parseInt(b.replace(/\D/g, '')));
		const paragraphs: string[] = [];
		for (const [i, name] of names.entries()) {
			paragraphs.push(`Slide ${i + 1}`);
			const doc = xml(await get(name));
			for (const p of Array.from(doc.getElementsByTagNameNS('*', 'p'))) {
				const line = Array.from(p.getElementsByTagNameNS('*', 't'))
					.map((t) => t.textContent ?? '')
					.join('');
				if (line.trim()) paragraphs.push(line);
			}
		}
		return { kind: 'text', paragraphs };
	}
	if (extension === 'xlsx') {
		const shared: string[] = [];
		if (entries.has('xl/sharedStrings.xml')) {
			const doc = xml(await get('xl/sharedStrings.xml'));
			for (const si of Array.from(doc.getElementsByTagNameNS('*', 'si'))) {
				shared.push(
					Array.from(si.getElementsByTagNameNS('*', 't'))
						.map((t) => t.textContent ?? '')
						.join('')
				);
			}
		}
		const sheets = Array.from(entries.keys()).filter((n) => /^xl\/worksheets\/sheet\d+\.xml$/.test(n)).sort();
		if (sheets.length === 0) throw new Error('the workbook has no sheet');
		const doc = xml(await get(sheets[0]));
		const rows: string[][] = [];
		for (const row of Array.from(doc.getElementsByTagNameNS('*', 'row')).slice(0, 500)) {
			const cells: string[] = [];
			for (const c of Array.from(row.getElementsByTagNameNS('*', 'c'))) {
				const at = columnIndex(c.getAttribute('r') ?? '');
				const v = c.getElementsByTagNameNS('*', 'v')[0]?.textContent ?? '';
				const inline = c.getElementsByTagNameNS('*', 't')[0]?.textContent;
				cells[at] = c.getAttribute('t') === 's' ? (shared[Number(v)] ?? '') : (inline ?? v);
			}
			rows.push(Array.from(cells, (x) => x ?? ''));
		}
		return { kind: 'table', rows, sheet: sheets[0].replace(/^.*\//, '') };
	}
	throw new Error('this kind of file is not previewed');
}

/**
 * A preview of Office files (docx, xlsx, pptx) that needs no library: these are zip archives of XML,
 * and the browser can inflate the entries itself (`DecompressionStream`). Words, simple formatting, tables
 * and cell values are shown, not the full layout. The result is plain data that the viewer draws with
 * components, never HTML from the file. Anything it cannot read throws, and the viewer falls back to a download.
 */

interface Entry {
	method: number;
	compressedSize: number;
	size: number;
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
		const size = view.getUint32(at + 24, true);
		const nameLen = view.getUint16(at + 28, true);
		const extraLen = view.getUint16(at + 30, true);
		const commentLen = view.getUint16(at + 32, true);
		const offset = view.getUint32(at + 42, true);
		const name = decoder.decode(bytes.subarray(at + 46, at + 46 + nameLen));
		entries.set(name, { method, compressedSize, size, offset });
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
	if (typeof DecompressionStream === 'undefined')
		throw new Error('this browser cannot unpack the file');
	const stream = new Blob([raw as BlobPart])
		.stream()
		.pipeThrough(new DecompressionStream('deflate-raw'));
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

export interface Run {
	text: string;
	bold?: boolean;
	italic?: boolean;
	underline?: boolean;
}
export type DocBlock =
	| { type: 'p'; level: number; list: boolean; runs: Run[] }
	| { type: 'table'; rows: DocBlock[][][] };
export interface Sheet {
	name: string;
	rows: string[][];
}
export type OfficePreview =
	| { kind: 'doc'; blocks: DocBlock[]; cut: boolean }
	| { kind: 'workbook'; sheets: Sheet[] }
	| { kind: 'slides'; slides: string[][] };

const MAX_BLOCKS = 2000;
const MAX_ROWS = 500;
const MAX_SHEETS = 20;

function local(node: Node): string {
	return (node as Element).localName ?? '';
}

function childrenNamed(parent: Element, name: string): Element[] {
	return Array.from(parent.children).filter((c) => c.localName === name);
}

function flag(props: Element | undefined, name: string): boolean {
	const el = props ? childrenNamed(props, name)[0] : undefined;
	if (!el) return false;
	const v = el.getAttribute('w:val') ?? el.getAttribute('val');
	return v !== '0' && v !== 'false' && v !== 'none';
}

function paragraph(p: Element): DocBlock {
	const props = childrenNamed(p, 'pPr')[0];
	const style = props ? (childrenNamed(props, 'pStyle')[0]?.getAttribute('w:val') ?? '') : '';
	const heading = /^(title)$/i.test(style) ? 1 : (/^heading\s*([1-6])$/i.exec(style)?.[1] ?? '0');
	const runs: Run[] = [];
	for (const r of Array.from(p.getElementsByTagNameNS('*', 'r'))) {
		const rp = childrenNamed(r, 'rPr')[0];
		let text = '';
		for (const n of Array.from(r.childNodes)) {
			const name = local(n);
			if (name === 't') text += n.textContent ?? '';
			else if (name === 'tab') text += '\t';
			else if (name === 'br') text += '\n';
		}
		if (text)
			runs.push({
				text,
				bold: flag(rp, 'b'),
				italic: flag(rp, 'i'),
				underline: flag(rp, 'u')
			});
	}
	return {
		type: 'p',
		level: Number(heading),
		list: !!props && childrenNamed(props, 'numPr').length > 0,
		runs
	};
}

function blocksOf(parent: Element, budget: { left: number }): DocBlock[] {
	const out: DocBlock[] = [];
	for (const node of Array.from(parent.children)) {
		if (budget.left <= 0) break;
		if (node.localName === 'p') {
			budget.left--;
			out.push(paragraph(node));
		} else if (node.localName === 'tbl') {
			budget.left--;
			const rows = childrenNamed(node, 'tr').map((tr) =>
				childrenNamed(tr, 'tc').map((tc) => blocksOf(tc, budget))
			);
			out.push({ type: 'table', rows });
		}
	}
	return out;
}

/** Sheet files in workbook order, with the names the author gave them. */
async function sheetList(
	entries: Map<string, Entry>,
	get: (name: string) => Promise<string>
): Promise<{ name: string; path: string }[]> {
	const files = Array.from(entries.keys())
		.filter((n) => /^xl\/worksheets\/sheet\d+\.xml$/.test(n))
		.sort((a, b) => parseInt(a.replace(/\D/g, '')) - parseInt(b.replace(/\D/g, '')));
	const fallback = files.map((path) => ({
		name: path.replace(/^.*\//, '').replace('.xml', ''),
		path
	}));
	if (!entries.has('xl/workbook.xml') || !entries.has('xl/_rels/workbook.xml.rels'))
		return fallback;
	try {
		const rels = new Map<string, string>();
		for (const r of Array.from(
			xml(await get('xl/_rels/workbook.xml.rels')).getElementsByTagNameNS('*', 'Relationship')
		)) {
			const target = r.getAttribute('Target') ?? '';
			rels.set(
				r.getAttribute('Id') ?? '',
				target.startsWith('/') ? target.slice(1) : `xl/${target}`
			);
		}
		const named: { name: string; path: string }[] = [];
		for (const s of Array.from(
			xml(await get('xl/workbook.xml')).getElementsByTagNameNS('*', 'sheet')
		)) {
			const path = rels.get(s.getAttribute('r:id') ?? '');
			if (path && entries.has(path)) named.push({ name: s.getAttribute('name') ?? path, path });
		}
		return named.length > 0 ? named : fallback;
	} catch {
		return fallback;
	}
}

export async function previewOffice(
	buffer: ArrayBuffer,
	extension: string
): Promise<OfficePreview> {
	const bytes = new Uint8Array(buffer);
	const entries = await readEntries(bytes);
	const get = async (name: string) => {
		const entry = entries.get(name);
		if (!entry) throw new Error(`${name} is missing`);
		return readText(bytes, entry);
	};
	if (extension === 'docx') {
		const body = xml(await get('word/document.xml')).getElementsByTagNameNS('*', 'body')[0];
		if (!body) throw new Error('the document has no body');
		const budget = { left: MAX_BLOCKS };
		const blocks = blocksOf(body, budget);
		return { kind: 'doc', blocks, cut: budget.left <= 0 };
	}
	if (extension === 'pptx') {
		const names = Array.from(entries.keys())
			.filter((n) => /^ppt\/slides\/slide\d+\.xml$/.test(n))
			.sort((a, b) => parseInt(a.replace(/\D/g, '')) - parseInt(b.replace(/\D/g, '')));
		const slides: string[][] = [];
		for (const name of names) {
			const lines: string[] = [];
			const doc = xml(await get(name));
			for (const p of Array.from(doc.getElementsByTagNameNS('*', 'p'))) {
				const line = Array.from(p.getElementsByTagNameNS('*', 't'))
					.map((t) => t.textContent ?? '')
					.join('');
				if (line.trim()) lines.push(line);
			}
			slides.push(lines);
		}
		return { kind: 'slides', slides };
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
		const list = (await sheetList(entries, get)).slice(0, MAX_SHEETS);
		if (list.length === 0) throw new Error('the workbook has no sheet');
		const sheets: Sheet[] = [];
		for (const { name, path } of list) {
			const doc = xml(await get(path));
			const rows: string[][] = [];
			for (const row of Array.from(doc.getElementsByTagNameNS('*', 'row')).slice(0, MAX_ROWS)) {
				const cells: string[] = [];
				for (const c of Array.from(row.getElementsByTagNameNS('*', 'c'))) {
					const at = columnIndex(c.getAttribute('r') ?? '');
					const v = c.getElementsByTagNameNS('*', 'v')[0]?.textContent ?? '';
					const inline = c.getElementsByTagNameNS('*', 't')[0]?.textContent;
					cells[at] = c.getAttribute('t') === 's' ? (shared[Number(v)] ?? '') : (inline ?? v);
				}
				rows.push(Array.from(cells, (x) => x ?? ''));
			}
			sheets.push({ name, rows });
		}
		return { kind: 'workbook', sheets };
	}
	throw new Error('this kind of file is not previewed');
}

export interface ArchiveEntry {
	name: string;
	size: number;
	packed: number;
}

/** The names and sizes inside a zip archive. Nothing is unpacked. */
export async function listArchive(buffer: ArrayBuffer, limit = 2000): Promise<ArchiveEntry[]> {
	const entries = await readEntries(new Uint8Array(buffer));
	return Array.from(entries, ([name, e]) => ({
		name,
		size: e.size,
		packed: e.compressedSize
	})).slice(0, limit);
}

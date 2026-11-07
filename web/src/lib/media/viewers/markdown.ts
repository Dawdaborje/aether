/**
 * A small Markdown reader that returns data, never HTML: the viewer draws it with components, so nothing in a
 * file can inject markup or script. Supports headings, paragraphs, lists, quotes, code blocks, rules, tables,
 * emphasis, inline code and links (http, https and mailto, and paths inside the app such as /wiki/p/abc).
 */
export type Inline =
	| { t: 'text'; v: string }
	| { t: 'code'; v: string }
	| { t: 'strong' | 'em' | 'del'; c: Inline[] }
	| { t: 'link'; href: string; c: Inline[]; app?: boolean };

export type Block =
	| { t: 'heading'; level: number; c: Inline[] }
	| { t: 'p'; c: Inline[] }
	| { t: 'list'; ordered: boolean; items: Inline[][] }
	| { t: 'quote'; c: Block[] }
	| { t: 'code'; lang: string; v: string }
	| { t: 'hr' }
	| { t: 'table'; head: Inline[][]; rows: Inline[][][] };

const SAFE_LINK = /^(https?:\/\/|mailto:)/i;
/** A page of the app itself: one slash, then a path. Never `//host` (another site) or a backslash. */
const APP_LINK = /^\/(?![/\\])[^\s\\]*$/;

export function parseInline(src: string, depth = 0): Inline[] {
	const out: Inline[] = [];
	let text = '';
	const flush = () => {
		if (text) out.push({ t: 'text', v: text });
		text = '';
	};
	let i = 0;
	while (i < src.length) {
		const rest = src.slice(i);
		let m: RegExpExecArray | null;
		if (src[i] === '\\' && i + 1 < src.length) {
			text += src[i + 1];
			i += 2;
		} else if ((m = /^`([^`]+)`/.exec(rest))) {
			flush();
			out.push({ t: 'code', v: m[1] });
			i += m[0].length;
		} else if (depth < 4 && (m = /^\*\*(.+?)\*\*|^__(.+?)__/.exec(rest))) {
			flush();
			out.push({ t: 'strong', c: parseInline(m[1] ?? m[2], depth + 1) });
			i += m[0].length;
		} else if (depth < 4 && (m = /^~~(.+?)~~/.exec(rest))) {
			flush();
			out.push({ t: 'del', c: parseInline(m[1], depth + 1) });
			i += m[0].length;
		} else if (depth < 4 && (m = /^\*([^*\s][^*]*?)\*|^_([^_\s][^_]*?)_(?!\w)/.exec(rest))) {
			flush();
			out.push({ t: 'em', c: parseInline(m[1] ?? m[2], depth + 1) });
			i += m[0].length;
		} else if (depth < 4 && (m = /^\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/.exec(rest))) {
			flush();
			const children = parseInline(m[1], depth + 1);
			// A link with an unsafe address (javascript:, data:) is shown as its text.
			if (SAFE_LINK.test(m[2])) out.push({ t: 'link', href: m[2], c: children });
			else if (APP_LINK.test(m[2])) out.push({ t: 'link', href: m[2], c: children, app: true });
			else out.push(...children);
			i += m[0].length;
		} else {
			text += src[i];
			i++;
		}
	}
	flush();
	return out;
}

function cells(line: string): string[] {
	return line
		.trim()
		.replace(/^\|/, '')
		.replace(/\|$/, '')
		.split('|')
		.map((c) => c.trim());
}

export function parseMarkdown(src: string, depth = 0): Block[] {
	const lines = src.replace(/\r\n?/g, '\n').split('\n');
	const blocks: Block[] = [];
	let i = 0;
	while (i < lines.length) {
		const line = lines[i];
		if (!line.trim()) {
			i++;
			continue;
		}
		let m: RegExpExecArray | null;
		if ((m = /^(```|~~~)\s*(\S*)/.exec(line))) {
			const fence = m[1];
			const body: string[] = [];
			i++;
			while (i < lines.length && !lines[i].startsWith(fence)) body.push(lines[i++]);
			i++;
			blocks.push({ t: 'code', lang: m[2], v: body.join('\n') });
		} else if ((m = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line))) {
			blocks.push({ t: 'heading', level: m[1].length, c: parseInline(m[2]) });
			i++;
		} else if (/^\s{0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) {
			blocks.push({ t: 'hr' });
			i++;
		} else if (line.startsWith('>')) {
			const body: string[] = [];
			while (i < lines.length && lines[i].startsWith('>'))
				body.push(lines[i++].replace(/^>\s?/, ''));
			blocks.push({ t: 'quote', c: depth < 4 ? parseMarkdown(body.join('\n'), depth + 1) : [] });
		} else if (/^\s*([-*+]|\d+[.)])\s+/.test(line)) {
			const ordered = /^\s*\d/.test(line);
			const items: Inline[][] = [];
			while (i < lines.length && /^\s*([-*+]|\d+[.)])\s+/.test(lines[i])) {
				items.push(parseInline(lines[i].replace(/^\s*([-*+]|\d+[.)])\s+/, '')));
				i++;
			}
			blocks.push({ t: 'list', ordered, items });
		} else if (
			line.includes('|') &&
			/^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(lines[i + 1] ?? '')
		) {
			const head = cells(line).map((c) => parseInline(c));
			i += 2;
			const rows: Inline[][][] = [];
			while (i < lines.length && lines[i].includes('|') && lines[i].trim()) {
				rows.push(cells(lines[i]).map((c) => parseInline(c)));
				i++;
			}
			blocks.push({ t: 'table', head, rows });
		} else {
			const para: string[] = [];
			while (
				i < lines.length &&
				lines[i].trim() &&
				!/^(#{1,6}\s|```|~~~|>|\s*([-*+]|\d+[.)])\s)/.test(lines[i])
			)
				para.push(lines[i++].trim());
			if (para.length === 0) {
				para.push(line);
				i++;
			}
			blocks.push({ t: 'p', c: parseInline(para.join(' ')) });
		}
	}
	return blocks;
}

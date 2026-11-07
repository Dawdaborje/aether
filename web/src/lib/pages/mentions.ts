/** Mentions in message text: finding them to highlight, and completing `@na` while typing. */
export interface Segment {
	text: string;
	mention: boolean;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Splits `body` into plain text and the `@Name` parts that name one of `names` (case-insensitive, ending at a word
 * boundary), or `@channel` / `@everyone`. Nothing is ever turned into markup: the parts are drawn as text.
 */
export function splitMentions(body: string, names: string[]): Segment[] {
	const words = [
		...names
			.filter(Boolean)
			.sort((a, b) => b.length - a.length)
			.map(escape),
		'channel',
		'everyone'
	];
	const pattern = new RegExp(`(?<![\\p{L}\\p{N}])@(?:${words.join('|')})(?![\\p{L}\\p{N}])`, 'giu');
	const out: Segment[] = [];
	let last = 0;
	for (const match of body.matchAll(pattern)) {
		const at = match.index ?? 0;
		if (at > last) out.push({ text: body.slice(last, at), mention: false });
		out.push({ text: match[0], mention: true });
		last = at + match[0].length;
	}
	if (last < body.length) out.push({ text: body.slice(last), mention: false });
	return out.length ? out : [{ text: body, mention: false }];
}

/**
 * The `@query` being typed just before `caret`, if any: `{ start, query }` where `start` is the position of the `@`.
 * An `@` counts only at the start of the text or after a space, so an email address does not open the list.
 */
export function mentionQuery(text: string, caret: number): { start: number; query: string } | null {
	const before = text.slice(0, caret);
	const at = before.lastIndexOf('@');
	if (at < 0) return null;
	if (at > 0 && !/\s/.test(before[at - 1])) return null;
	const query = before.slice(at + 1);
	// A name may hold a space ("Ann Lee"), but a line break or a long run ends the search.
	if (query.includes('\n') || query.length > 30) return null;
	return { start: at, query };
}

/** Put `name` where the `@query` was, and a space after it; returns the new text and where the caret goes. */
export function applyMention(
	text: string,
	caret: number,
	start: number,
	name: string
): { text: string; caret: number } {
	const insert = `@${name} `;
	return { text: text.slice(0, start) + insert + text.slice(caret), caret: start + insert.length };
}

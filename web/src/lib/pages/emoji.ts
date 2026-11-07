/**
 * The logic of the composer's emoji picker, apart from the drawing. It holds no emoji of its own: the plugin that
 * owns the composer's page answers the pack (see `emoji` on `<composer>`), so what is offered, and in which
 * language, is the plugin's choice.
 */
export interface PackEmoji {
	/** The character(s). */
	c: string;
	/** Its name. */
	n: string;
	/** Words it can be found by. */
	t?: string[];
	/** `:shortcodes` that type it. */
	s?: string[];
	/** Index into `groups`. */
	g: number;
	/** The five skin-tone versions, light to dark, for people and hands. */
	k?: string[];
}

export interface EmojiPack {
	groups: { key: string; name: string }[];
	emoji: PackEmoji[];
}

export function isPack(value: unknown): value is EmojiPack {
	const v = value as Partial<EmojiPack> | null;
	return !!v && Array.isArray(v.groups) && Array.isArray(v.emoji);
}

/** The emoji with a skin tone applied (1 to 5; 0 or an emoji without tones is left as it is). */
export function toned(emoji: PackEmoji, tone: number): string {
	return (tone >= 1 && tone <= 5 && emoji.k?.[tone - 1]) || emoji.c;
}

/** The emoji whose name, words or shortcodes contain `query`; names that start with it come first. */
export function searchPack(pack: EmojiPack, query: string, limit = 200): PackEmoji[] {
	const q = query.trim().toLowerCase();
	if (!q) return [];
	const rank = (e: PackEmoji): number => {
		if (e.s?.some((s) => s.startsWith(q))) return 0;
		if (e.n.startsWith(q)) return 1;
		if (e.n.includes(q) || e.s?.some((s) => s.includes(q))) return 2;
		if (e.t?.some((t) => t.startsWith(q))) return 3;
		return -1;
	};
	return pack.emoji
		.map((e, i) => ({ e, i, r: rank(e) }))
		.filter((x) => x.r >= 0)
		.sort((a, b) => a.r - b.r || a.i - b.i)
		.slice(0, limit)
		.map((x) => x.e);
}

/** The emoji of one group, in the order of the pack. */
export function inGroup(pack: EmojiPack, group: number): PackEmoji[] {
	return pack.emoji.filter((e) => e.g === group);
}

const byCode = new WeakMap<EmojiPack, Map<string, PackEmoji>>();

function codes(pack: EmojiPack): Map<string, PackEmoji> {
	let map = byCode.get(pack);
	if (!map) {
		map = new Map();
		for (const e of pack.emoji) for (const s of e.s ?? []) if (!map.has(s)) map.set(s, e);
		byCode.set(pack, map);
	}
	return map;
}

/** Shortcodes that start with `query` first, then those that contain it; at most `limit`. */
export function searchShortcodes(
	pack: EmojiPack,
	query: string,
	limit = 8
): { name: string; emoji: PackEmoji }[] {
	const q = query.toLowerCase();
	const all = [...codes(pack)].map(([name, emoji]) => ({ name, emoji }));
	const starts = all.filter((x) => x.name.startsWith(q));
	const has = all.filter((x) => !x.name.startsWith(q) && x.name.includes(q));
	return [...starts, ...has].slice(0, limit);
}

/** A `:name` being typed just before `caret`: it must start the text or follow whitespace, and have 2+ letters. */
export function shortcodeQuery(text: string, caret: number): { start: number; query: string } | null {
	const before = text.slice(0, caret);
	const m = /(^|\s):([a-z0-9_+-]{2,})$/i.exec(before);
	if (!m) return null;
	return { start: caret - m[2].length - 1, query: m[2].toLowerCase() };
}

/** `:smile:` just completed before `caret`: the emoji and where the code starts, or null. */
export function completedShortcode(
	pack: EmojiPack,
	text: string,
	caret: number
): { start: number; emoji: string } | null {
	const m = /(^|\s):([a-z0-9_+-]{2,}):$/i.exec(text.slice(0, caret));
	const found = m ? codes(pack).get(m[2].toLowerCase()) : undefined;
	return m && found ? { start: caret - m[2].length - 2, emoji: found.c } : null;
}

/** `emoji` put into `text` at the selection (replacing it); the new text and where the cursor goes. */
export function insertAt(
	text: string,
	start: number,
	end: number,
	emoji: string
): { text: string; caret: number } {
	const from = Math.max(0, Math.min(start, text.length));
	const to = Math.max(from, Math.min(end, text.length));
	return { text: text.slice(0, from) + emoji + text.slice(to), caret: from + emoji.length };
}

/**
 * The packs of several plugins as one: groups with the same `key` are joined (so two plugins adding "flags" share a
 * tab), and an emoji already offered by an earlier pack is not offered twice.
 */
export function mergePacks(packs: EmojiPack[]): EmojiPack {
	const groups: EmojiPack['groups'] = [];
	const emoji: PackEmoji[] = [];
	const seen = new Set<string>();
	for (const pack of packs) {
		const at = pack.groups.map((g) => {
			let i = groups.findIndex((x) => x.key === g.key);
			if (i < 0) i = groups.push({ key: g.key, name: g.name }) - 1;
			return i;
		});
		for (const e of pack.emoji) {
			if (seen.has(e.c) || at[e.g] === undefined) continue;
			seen.add(e.c);
			emoji.push({ ...e, g: at[e.g] });
		}
	}
	return { groups, emoji };
}

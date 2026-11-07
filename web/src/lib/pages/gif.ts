/**
 * The logic of the composer's GIF picker, apart from the drawing. It holds no GIF of its own: the plugins that
 * register a `[[gif_pack]]` ship them (see `gifRegistry.ts`), so what is offered is the plugins' choice.
 */
export interface PackGif {
	/** Its name. */
	n: string;
	/** The file, relative to the plugin's `assets/` folder. */
	f: string;
	/** Words it can be found by. */
	t?: string[];
	/** `:shortcodes` it is also known by. */
	s?: string[];
}

export interface GifPack {
	gifs: PackGif[];
}

/** A GIF ready to show: the pack's entry with the address of its file and the plugin that ships it. */
export interface Gif {
	name: string;
	url: string;
	plugin: string;
	words: string[];
}

export function isGifPack(value: unknown): value is GifPack {
	const v = value as Partial<GifPack> | null;
	return !!v && Array.isArray(v.gifs);
}

/** Where a plugin's asset is served; null for a path that is not plainly inside `assets/`. */
export function gifUrl(plugin: string, file: string): string | null {
	const clean =
		file !== '' &&
		!file.startsWith('/') &&
		!file.includes('\\') &&
		!file.includes(':') &&
		file.split('/').every((part) => part !== '' && part !== '.' && part !== '..');
	return clean && /^[a-z0-9_-]+$/.test(plugin)
		? `/api/ui/plugins/${plugin}/assets/${file.split('/').map(encodeURIComponent).join('/')}`
		: null;
}

/** The GIFs of several packs as one list; a GIF with an unsafe path, or one already listed, is left out. */
export function mergeGifs(packs: { plugin: string; pack: GifPack }[]): Gif[] {
	const seen = new Set<string>();
	const out: Gif[] = [];
	for (const { plugin, pack } of packs) {
		for (const g of pack.gifs) {
			const url = typeof g.f === 'string' ? gifUrl(plugin, g.f) : null;
			if (!url || typeof g.n !== 'string' || seen.has(url)) continue;
			seen.add(url);
			out.push({ name: g.n, url, plugin, words: [...(g.t ?? []), ...(g.s ?? [])] });
		}
	}
	return out;
}

/** The GIFs whose name or words match `query`, names that start with it first; all of them for an empty query. */
export function searchGifs(gifs: Gif[], query: string, limit = 120): Gif[] {
	const q = query.trim().toLowerCase();
	if (!q) return gifs.slice(0, limit);
	const rank = (g: Gif): number => {
		const name = g.name.toLowerCase();
		if (name.startsWith(q) || g.words.some((w) => w.toLowerCase() === q)) return 0;
		if (name.includes(q)) return 1;
		if (g.words.some((w) => w.toLowerCase().startsWith(q))) return 2;
		return -1;
	};
	return gifs
		.map((g, i) => ({ g, i, r: rank(g) }))
		.filter((x) => x.r >= 0)
		.sort((a, b) => a.r - b.r || a.i - b.i)
		.slice(0, limit)
		.map((x) => x.g);
}

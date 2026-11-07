import { isGifPack, mergeGifs, type Gif, type GifPack } from './gif';
import { packRegistry } from './packRegistry';

/** The GIF registry (`[[gif_pack]]` of every enabled plugin, `GET /api/ui/gifs`), apart from the emoji one. */
const registry = packRegistry<GifPack>('gif', '/api/ui/gifs', isGifPack);

/**
 * The GIFs to offer. With no `only`, every registered pack; with `only`, the packs of that name
 * (`gif="noto-animated"` on a `<composer>`). Null when nothing is registered.
 */
export async function loadGifs(only?: string): Promise<Gif[] | null> {
	const found = await registry.load(only);
	return found.length > 0
		? mergeGifs(found.map((x) => ({ plugin: x.listed.plugin, pack: x.pack })))
		: null;
}

export const resetGifs = registry.reset;

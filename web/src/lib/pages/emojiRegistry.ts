import { isPack, mergePacks, type EmojiPack } from './emoji';
import { packRegistry } from './packRegistry';

/** The emoji registry (`[[emoji_pack]]` of every enabled plugin, `GET /api/ui/emoji`). */
const registry = packRegistry<EmojiPack>('emoji', '/api/ui/emoji', isPack);

/**
 * The emoji to offer. With no `only`, every registered pack, merged; with `only`, the packs of that name
 * (`emoji="emojibase"` on a `<composer>`). Null when nothing is registered.
 */
export async function loadEmoji(only?: string): Promise<EmojiPack | null> {
	const found = await registry.load(only);
	return found.length > 0 ? mergePacks(found.map((x) => x.pack)) : null;
}

export const resetEmoji = registry.reset;

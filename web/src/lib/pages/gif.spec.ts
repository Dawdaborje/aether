import { describe, expect, it } from 'vitest';
import { gifUrl, isGifPack, mergeGifs, searchGifs, type Gif } from './gif';

const gifs: Gif[] = [
	{ name: 'grinning face', url: '/a', plugin: 'p', words: ['happy', 'smile'] },
	{ name: 'party popper', url: '/b', plugin: 'p', words: ['celebrate', 'tada'] },
	{ name: 'smiling face', url: '/c', plugin: 'p', words: ['happy'] }
];

describe('gif', () => {
	it('knows a pack when it sees one', () => {
		expect(isGifPack({ gifs: [] })).toBe(true);
		expect(isGifPack({ emoji: [] })).toBe(false);
		expect(isGifPack(null)).toBe(false);
	});

	it('builds the address of an asset, and refuses a path that leaves assets/', () => {
		expect(gifUrl('atrium_chat', 'gif/1f600.gif')).toBe('/api/ui/plugins/atrium_chat/assets/gif/1f600.gif');
		expect(gifUrl('atrium_chat', 'gif/a b.gif')).toBe('/api/ui/plugins/atrium_chat/assets/gif/a%20b.gif');
		for (const bad of ['../x.gif', '/x.gif', 'gif//x.gif', 'a\\b.gif', 'https://x/y.gif', '']) {
			expect(gifUrl('atrium_chat', bad), bad).toBeNull();
		}
		expect(gifUrl('Bad Plugin', 'a.gif')).toBeNull();
	});

	it('merges packs, leaving out unsafe and repeated GIFs', () => {
		const merged = mergeGifs([
			{ plugin: 'p', pack: { gifs: [{ n: 'one', f: 'g/1.gif', t: ['x'] }, { n: 'bad', f: '../2.gif' }] } },
			{ plugin: 'p', pack: { gifs: [{ n: 'again', f: 'g/1.gif' }, { n: 'two', f: 'g/2.gif', s: ['two'] }] } }
		]);
		expect(merged.map((g) => g.name)).toEqual(['one', 'two']);
		expect(merged[0].words).toEqual(['x']);
		expect(merged[1].words).toEqual(['two']);
	});

	it('searches names and words, best matches first', () => {
		expect(searchGifs(gifs, 'happy').map((g) => g.url)).toEqual(['/a', '/c']);
		expect(searchGifs(gifs, 'smil').map((g) => g.url)).toEqual(['/c', '/a']);
		expect(searchGifs(gifs, 'tada').map((g) => g.url)).toEqual(['/b']);
		expect(searchGifs(gifs, 'zzz')).toEqual([]);
		expect(searchGifs(gifs, '').length).toBe(3);
	});
});

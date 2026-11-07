import { describe, expect, it } from 'vitest';
import {
	completedShortcode,
	inGroup,
	insertAt,
	isPack,
	searchPack,
	searchShortcodes,
	shortcodeQuery,
	toned,
	type EmojiPack
} from './emoji';

const pack: EmojiPack = {
	groups: [
		{ key: 'smileys', name: 'smileys' },
		{ key: 'people', name: 'people' }
	],
	emoji: [
		{ c: '😀', n: 'grinning face', t: ['happy', 'grin'], s: ['grinning'], g: 0 },
		{ c: '😄', n: 'grinning face with smiling eyes', t: ['smile'], s: ['smile'], g: 0 },
		{ c: '🙂', n: 'slightly smiling face', s: ['slight_smile'], g: 0 },
		{ c: '👍', n: 'thumbs up', t: ['like'], s: ['+1', 'thumbsup'], g: 1, k: ['👍🏻', '👍🏼', '👍🏽', '👍🏾', '👍🏿'] }
	]
};

describe('emoji', () => {
	it('inserts at the cursor and replaces a selection', () => {
		expect(insertAt('hello world', 5, 5, '👍')).toEqual({ text: 'hello👍 world', caret: 7 });
		expect(insertAt('hello world', 6, 11, '🎉')).toEqual({ text: 'hello 🎉', caret: 8 });
		expect(insertAt('hi', 99, 99, '🙂').text).toBe('hi🙂');
	});

	it('knows a pack when it sees one', () => {
		expect(isPack(pack)).toBe(true);
		expect(isPack({ groups: [] })).toBe(false);
		expect(isPack(null)).toBe(false);
	});

	it('applies a skin tone only where there is one', () => {
		expect(toned(pack.emoji[3], 3)).toBe('👍🏽');
		expect(toned(pack.emoji[3], 0)).toBe('👍');
		expect(toned(pack.emoji[0], 4)).toBe('😀');
	});

	it('searches names, words and shortcodes, best matches first', () => {
		expect(searchPack(pack, 'smile').map((e) => e.c)).toEqual(['😄', '🙂']);
		expect(searchPack(pack, 'grin').map((e) => e.c)).toEqual(['😀', '😄']);
		expect(searchPack(pack, 'like').map((e) => e.c)).toEqual(['👍']);
		expect(searchPack(pack, '')).toEqual([]);
	});

	it('lists the emoji of a group', () => {
		expect(inGroup(pack, 1).map((e) => e.c)).toEqual(['👍']);
	});
});

describe('shortcodes', () => {
	it('sees a name being typed after whitespace, not inside a word or a time', () => {
		expect(shortcodeQuery('hi :smi', 7)).toEqual({ start: 3, query: 'smi' });
		expect(shortcodeQuery(':sm', 3)).toEqual({ start: 0, query: 'sm' });
		expect(shortcodeQuery('at 10:30', 8)).toBeNull();
		expect(shortcodeQuery('a :s', 4)).toBeNull();
	});

	it('lists names that start with the query before those containing it', () => {
		const names = searchShortcodes(pack, 'smi').map((x) => x.name);
		expect(names).toEqual(['smile', 'slight_smile']);
		expect(searchShortcodes(pack, 'zzzz')).toEqual([]);
	});

	it('turns a closed known code into its emoji', () => {
		expect(completedShortcode(pack, 'ok :smile:', 10)).toEqual({ start: 3, emoji: '😄' });
		expect(completedShortcode(pack, 'ok :nonsense:', 13)).toBeNull();
	});
});

import { mergePacks } from './emoji';

describe('mergePacks', () => {
	const a: EmojiPack = { groups: [{ key: 'smileys', name: 'smileys' }], emoji: [{ c: '😀', n: 'grinning', g: 0 }] };
	const b: EmojiPack = {
		groups: [
			{ key: 'flags', name: 'flags' },
			{ key: 'smileys', name: 'faces' }
		],
		emoji: [
			{ c: '🏳️', n: 'white flag', g: 0 },
			{ c: '😀', n: 'again', g: 1 },
			{ c: '🙂', n: 'slight', g: 1 }
		]
	};

	it('joins groups with the same key and renumbers the emoji', () => {
		const merged = mergePacks([a, b]);
		expect(merged.groups.map((g) => g.key)).toEqual(['smileys', 'flags']);
		expect(merged.emoji.map((e) => [e.c, e.g])).toEqual([['😀', 0], ['🏳️', 1], ['🙂', 0]]);
	});

	it('offers an emoji once, from the first pack that has it', () => {
		expect(mergePacks([a, b]).emoji.filter((e) => e.c === '😀')[0].n).toBe('grinning');
		expect(mergePacks([])).toEqual({ groups: [], emoji: [] });
	});
});
